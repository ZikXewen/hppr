#![windows_subsystem = "windows"]
use std::collections::HashSet;

use bevy::prelude::*;
use bevy_asset_loader::{AssetCollection, AssetLoader};
use bevy_ecs_ldtk::prelude::*;
use heron::prelude::*;
use iyes_loopless::prelude::*;

#[derive(Clone, Eq, PartialEq, Debug, Hash)]
enum GameState {
    AssetLoading,
    Playing,
}

fn main() {
    let mut app = App::new();
    app.add_loopless_state(GameState::AssetLoading);
    AssetLoader::new(GameState::AssetLoading)
        .continue_to_state(GameState::Playing)
        .with_collection::<ImageAssets>()
        .build(&mut app);
    app.add_plugins(DefaultPlugins)
        .add_plugin(PhysicsPlugin::default())
        .add_plugin(LdtkPlugin)
        .add_enter_system(GameState::Playing, setup)
        .insert_resource(Gravity::from(Vec2::new(0.0, -1000.)))
        .insert_resource(LevelSelection::Index(0))
        .add_system(fit_camera)
        .add_system(pause_physics_during_load)
        .add_system(ground_detection)
        .add_system(ladder_and_pad_detection)
        .add_system(movement)
        .register_ldtk_int_cell_for_layer::<WallBundle>("Colliders", 1)
        .register_ldtk_int_cell_for_layer::<LadderBundle>("Tiles", 2)
        .register_ldtk_int_cell_for_layer::<PadBundle>("Tiles", 3)
        .register_ldtk_entity::<PlayerBundle>("Player")
        .run();
}

#[derive(AssetCollection)]
struct ImageAssets {
    #[asset(path = "map.ldtk")]
    map: Handle<LdtkAsset>,
}

fn setup(mut commands: Commands, images: Res<ImageAssets>) {
    commands.spawn_bundle(OrthographicCameraBundle::new_2d());
    commands.spawn_bundle(LdtkWorldBundle {
        ldtk_handle: images.map.clone(),
        ..Default::default()
    });
}

fn fit_camera(
    mut camera: Query<&mut OrthographicProjection>,
    ldtk_levels: Res<Assets<LdtkLevel>>,
    current_level: Res<LevelSelection>,
    windows: Res<Windows>,
) {
    let window = windows.primary();
    let window_aspect = window.width() / window.height();
    if let Ok(mut camera_projection) = camera.get_single_mut() {
        if let Some((_, LdtkLevel { level, .. })) = ldtk_levels
            .iter()
            .find(|(_, ldtk_level)| current_level.is_match(&0, &ldtk_level.level))
        {
            camera_projection.scaling_mode = bevy::render::camera::ScalingMode::None;
            camera_projection.bottom = 0.;
            camera_projection.left = 0.;
            let level_aspect = level.px_wid as f32 / level.px_hei as f32;
            if level_aspect < window_aspect {
                camera_projection.right = (level.px_hei as f32 * window_aspect).round();
                camera_projection.top = level.px_hei as f32;
            } else {
                camera_projection.top = (level.px_wid as f32 / window_aspect).round();
                camera_projection.right = level.px_wid as f32;
            }
        }
    }
}

#[derive(Default, Bundle)]
struct CollisionBundle {
    collision_shape: CollisionShape,
    rigid_body: RigidBody,
    rotation_constraints: RotationConstraints,
    physic_material: PhysicMaterial,
}
impl From<IntGridCell> for CollisionBundle {
    fn from(int_grid_cell: IntGridCell) -> Self {
        match int_grid_cell.value {
            1 => Self {
                collision_shape: CollisionShape::Cuboid {
                    half_extends: Vec3::new(9., 9., 0.),
                    border_radius: None,
                },
                rigid_body: RigidBody::Static,
                physic_material: PhysicMaterial {
                    friction: 0.1,
                    ..Default::default()
                },
                ..Default::default()
            },
            2 => Self {
                collision_shape: CollisionShape::Cuboid {
                    half_extends: Vec3::new(9., 9., 0.),
                    border_radius: None,
                },
                rigid_body: RigidBody::Sensor,
                ..Default::default()
            },
            3 => Self {
                collision_shape: CollisionShape::Cuboid {
                    half_extends: Vec3::new(9., 5., 0.),
                    border_radius: None,
                },
                rigid_body: RigidBody::Sensor,
                ..Default::default()
            },
            _ => Default::default(),
        }
    }
}
impl From<EntityInstance> for CollisionBundle {
    fn from(_: EntityInstance) -> Self {
        Self {
            collision_shape: CollisionShape::Cuboid {
                half_extends: Vec3::new(10., 12., 0.),
                border_radius: None,
            },
            rotation_constraints: RotationConstraints::lock(),
            ..Default::default()
        }
    }
}

#[derive(Default, Component)]
struct Pad;

#[derive(Bundle, LdtkIntCell)]
struct PadBundle {
    pad: Pad,
    #[from_int_grid_cell]
    #[bundle]
    collision_bundle: CollisionBundle,
}

#[derive(Default, Component)]
struct Ladder;

#[derive(Bundle, LdtkIntCell)]
struct LadderBundle {
    ladder: Ladder,
    #[from_int_grid_cell]
    #[bundle]
    collision_bundle: CollisionBundle,
}

#[derive(Default, Component)]
struct Wall;

#[derive(Bundle, LdtkIntCell)]
struct WallBundle {
    wall: Wall,
    #[from_int_grid_cell]
    #[bundle]
    collision_bundle: CollisionBundle,
}

#[derive(Default, Component)]
struct Player {
    on_ground: bool,
    on_ladder: bool,
    over_ladders: HashSet<Entity>,
}

#[derive(Bundle, LdtkEntity)]
struct PlayerBundle {
    #[sprite_sheet_bundle("characters.png", 24., 24., 9, 3, 0., 2)]
    #[bundle]
    sprite_bundle: SpriteSheetBundle,
    player: Player,
    #[from_entity_instance]
    #[bundle]
    collision_bundle: CollisionBundle,
    velocity: Velocity,
}

fn pause_physics_during_load(
    mut level_events: EventReader<LevelEvent>,
    mut physics_time: ResMut<PhysicsTime>,
) {
    for event in level_events.iter() {
        match event {
            LevelEvent::SpawnTriggered(_) => physics_time.set_scale(0.),
            LevelEvent::Transformed(_) => physics_time.set_scale(1.),
            _ => (),
        }
    }
}

fn ground_detection(
    mut player: Query<(&Transform, &mut Player)>,
    physics_world: heron::rapier_plugin::PhysicsWorld,
) {
    if let Ok((transform, mut player)) = player.get_single_mut() {
        player.on_ground = physics_world
            .ray_cast(
                transform.translation + Vec3::new(9., -12.1, 0.),
                Vec3::new(0., -2., 0.),
                true,
            )
            .is_some()
            || physics_world
                .ray_cast(
                    transform.translation + Vec3::new(-9., -12.1, 0.),
                    Vec3::new(0., -2., 0.),
                    true,
                )
                .is_some();
    }
}

fn ladder_and_pad_detection(
    mut player: Query<(&mut Velocity, &mut Player, Entity)>,
    pads: Query<&Pad>,
    ladders: Query<&Ladder>,
    mut collisions: EventReader<CollisionEvent>,
) {
    for col in collisions.iter() {
        if let Ok((mut velocity, mut player, entity)) = player.get_single_mut() {
            match col {
                CollisionEvent::Started(a, b) if entity == a.rigid_body_entity() => {
                    if ladders.contains(b.rigid_body_entity()) {
                        player.over_ladders.insert(b.rigid_body_entity());
                    }
                    if pads.contains(b.rigid_body_entity()) {
                        velocity.linear.y = 350.;
                    }
                }
                CollisionEvent::Started(a, b) if entity == b.rigid_body_entity() => {
                    if ladders.contains(a.rigid_body_entity()) {
                        player.over_ladders.insert(a.rigid_body_entity());
                    }
                    if pads.contains(a.rigid_body_entity()) {
                        velocity.linear.y = 350.;
                    }
                }
                CollisionEvent::Stopped(a, b) if entity == a.rigid_body_entity() => {
                    if ladders.contains(b.rigid_body_entity()) {
                        player.over_ladders.remove(&b.rigid_body_entity());
                    }
                }
                CollisionEvent::Stopped(a, b) if entity == b.rigid_body_entity() => {
                    if ladders.contains(a.rigid_body_entity()) {
                        player.over_ladders.remove(&a.rigid_body_entity());
                    }
                }
                _ => (),
            }
        }
    }
}

fn movement(input: Res<Input<KeyCode>>, mut player: Query<(&mut Velocity, &mut Player)>) {
    if let Ok((mut velocity, mut player)) = player.get_single_mut() {
        use KeyCode::*;
        let right = if input.pressed(D) { 1. } else { 0. };
        let left = if input.pressed(A) { 1. } else { 0. };
        velocity.linear.x = (right - left) * 100.;
        if player.on_ladder {
            let up = if input.pressed(W) { 1. } else { 0. };
            let down = if input.pressed(S) { 1. } else { 0. };
            velocity.linear.y = (up - down) * 100.;
        }
        if (player.on_ground || player.on_ladder) && input.just_pressed(Space) {
            velocity.linear.y = 200.;
            player.on_ladder = false;
        }
        if player.over_ladders.is_empty() {
            player.on_ladder = false;
        } else if input.pressed(W) || input.pressed(S) {
            player.on_ladder = true;
        }
    }
}
