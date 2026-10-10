//! Numeric checks of the movement rules, using Qui-Gon's values from his character file.

use crate::clips::{Action, ClipInfo, Clips};
use crate::player::{Context, Event, Input, JumpState, Player, Stats};

const DT: f32 = 1.0 / 60.0;

fn stats() -> Stats {
    Stats {
        tiptoe_speed: 0.153867,
        walk_speed: 0.6,
        run_speed: 1.2,
        gravity: -6.0,
        jump_speed: 2.3,
        jump_2_speed: 3.0,
        acceleration: 10.0,
        ..Stats::default()
    }
}

fn clips() -> Clips {
    let mut clips = Clips::default();
    let clip = |length: f32, looping: bool, blend_in: f32, speed: f32, frames: [f32; 4]| ClipInfo {
        file: String::new(),
        length,
        fps: 30.0,
        looping,
        blend_in,
        blend_out: 0.2,
        speed,
        frames,
    };
    for action in Action::all() {
        let info = match action {
            Action::Idle | Action::WeaponIdle | Action::Walk | Action::Run | Action::Tiptoe | Action::Fall => {
                clip(29.0, true, 0.2, 0.0, [0.0; 4])
            }
            Action::Lunge => clip(20.0, true, 0.2, 1.5, [0.0; 4]),
            Action::WeaponOut => clip(30.0, false, 0.2, 0.0, [12.0, 18.0, 0.0, 0.0]),
            Action::WeaponIn => clip(20.0, false, 0.2, 0.0, [0.0, 12.0, 0.0, 0.0]),
            Action::Combo(_, 0) => clip(18.0, false, 0.2, 0.6, [11.25, 13.0, 0.0, 0.0]),
            Action::Combo(_, _) => clip(22.0, false, 0.0, 0.6, [12.0, 14.0, 0.0, 0.0]),
            _ => clip(15.0, false, 0.0, 0.0, [0.0; 4]),
        };
        clips.insert(action, info);
    }
    clips
}

fn run(player: &mut Player, clips: &Clips, input: Input, frames: usize) -> Vec<Event> {
    let mut events = Vec::new();
    for i in 0..frames {
        let mut frame_input = input;
        if i > 0 {
            frame_input.jump_pressed = false;
            frame_input.action_pressed = false;
            frame_input.special_pressed = false;
        }
        player.step(&frame_input, 0, clips, DT);
        events.extend(player.events.iter().copied());
    }
    events
}

fn forward() -> Input {
    Input { stick_z: 1.0, ..Default::default() }
}

#[test]
fn run_reaches_top_speed_and_stops() {
    let (clips, mut player) = (clips(), Player::new(stats()));
    run(&mut player, &clips, forward(), 120);
    assert!((player.speed() - 1.2).abs() < 0.01, "speed {}", player.speed());
    assert!(player.pos.z > 2.0 && player.pos.x.abs() < 1e-3);
    assert_eq!(player.anim.action, Action::Run);
    // Velocity closes 10 * dt of the gap per frame: after 0.2 s about 13% is left.
    run(&mut player, &clips, Input::default(), 12);
    assert!((player.speed() - 1.2 * (1.0f32 - 10.0 * DT).powi(12)).abs() < 0.01);
    run(&mut player, &clips, Input::default(), 120);
    assert!(player.speed() < 0.001);
    assert_eq!(player.anim.action, Action::Idle);
}

#[test]
fn stick_tiers() {
    let clips = clips();
    for (deflection, speed, action) in [(0.4, 0.153867, Action::Tiptoe), (0.7, 0.6, Action::Walk), (1.0, 1.2, Action::Run)] {
        let mut player = Player::new(stats());
        run(&mut player, &clips, Input { stick_z: deflection, ..Default::default() }, 180);
        assert!((player.speed() - speed).abs() < 0.01, "{deflection}: {}", player.speed());
        assert_eq!(player.anim.action, action);
    }
    let mut player = Player::new(stats());
    run(&mut player, &clips, Input { stick_z: 0.3, ..Default::default() }, 60);
    assert_eq!(player.speed(), 0.0, "inside the dead zone");
}

#[test]
fn stick_is_camera_relative() {
    let (clips, mut player) = (clips(), Player::new(stats()));
    for _ in 0..120 {
        player.step(&forward(), 0x4000, &clips, DT);
    }
    assert!(player.pos.x > 2.0 && player.pos.z.abs() < 0.01, "{:?}", player.pos);
    assert!((player.facing as i32 - 0x4000).abs() < 16);
}

#[test]
fn single_jump_height_and_time() {
    let (clips, mut player) = (clips(), Player::new(stats()));
    run(&mut player, &clips, Input { jump_pressed: true, jump_held: true, ..Default::default() }, 1);
    assert_eq!(player.context, Context::Jump);
    let (mut peak, mut frames) = (0.0f32, 1);
    while player.context == Context::Jump && frames < 600 {
        run(&mut player, &clips, Input::default(), 1);
        peak = peak.max(player.pos.y);
        frames += 1;
    }
    // v^2 / (2 g) = 2.3^2 / 12 = 0.4408; air time 2 v / g = 0.767 s.
    assert!((peak - 0.4408).abs() < 0.03, "peak {peak}");
    assert!((frames as f32 * DT - 0.767).abs() < 0.06, "air time {}", frames as f32 * DT);
    assert_eq!(player.context, Context::LandJump);
}

#[test]
fn double_jump_reaches_second_height_then_slam() {
    let (clips, mut player) = (clips(), Player::new(stats()));
    let jump = Input { jump_pressed: true, jump_held: true, ..Default::default() };
    run(&mut player, &clips, jump, 10);
    let events = run(&mut player, &clips, jump, 1);
    assert!(events.contains(&Event::Jump { second: true }));
    assert_eq!(player.jump_sequence, 2);
    let mut peak = 0.0f32;
    for _ in 0..26 {
        run(&mut player, &clips, Input::default(), 1);
        peak = peak.max(player.pos.y);
    }
    // The slam is only available near the top of the arc (falling slower than 1.25).
    // The double jump tops out at the second jump's height above the take-off point: 3^2 / 12.
    assert!((peak - 0.75).abs() < 0.03, "peak {peak}");
    run(&mut player, &clips, Input { action_pressed: true, ..Default::default() }, 1);
    assert_eq!(player.jump_state, JumpState::Slam);
    let events = run(&mut player, &clips, Input::default(), 120);
    assert!(events.contains(&Event::Judder(0.5)));
    assert!(player.on_ground);
}

#[test]
fn attack_in_single_jump_is_a_lunge() {
    let (clips, mut player) = (clips(), Player::new(stats()));
    run(&mut player, &clips, Input { jump_pressed: true, jump_held: true, ..Default::default() }, 8);
    run(&mut player, &clips, Input { action_pressed: true, ..Default::default() }, 1);
    assert_eq!((player.context, player.jump_state), (Context::Jump, JumpState::Lunge));
    run(&mut player, &clips, Input::default(), 20);
    assert!(player.speed() > 1.3, "lunge travels at 1.5: {}", player.speed());
    run(&mut player, &clips, Input::default(), 90);
    assert!(player.on_ground && player.pos.z > 0.5);
}

#[test]
fn reversing_then_jumping_is_a_backflip() {
    let (clips, mut player) = (clips(), Player::new(stats()));
    run(&mut player, &clips, forward(), 60);
    let back = Input { stick_z: -1.0, ..Default::default() };
    run(&mut player, &clips, back, 3);
    run(&mut player, &clips, Input { jump_pressed: true, jump_held: true, ..back }, 1);
    assert_eq!((player.context, player.jump_state), (Context::Jump, JumpState::Flip));
    run(&mut player, &clips, back, 20);
    assert!(player.vel.z < -0.5, "travels backwards");
    assert!((player.facing as i16).abs() < 0x1000, "still facing forwards: {:#x}", player.facing);
}

#[test]
fn saber_draw_then_three_hit_combo() {
    let (clips, mut player) = (clips(), Player::new(stats()));
    let attack = Input { action_pressed: true, ..Default::default() };
    let events = run(&mut player, &clips, attack, 1);
    assert!(events.contains(&Event::SaberOn));
    assert_eq!(player.context, Context::WeaponOut);
    run(&mut player, &clips, Input::default(), 90);
    assert!(player.weapon_out && player.blade_scale == 1.0);
    assert_eq!(player.anim.action, Action::WeaponIdle);

    run(&mut player, &clips, attack, 1);
    assert_eq!(player.context, Context::Combo);
    assert_eq!(player.context_anim, Action::Combo(2, 0));
    let mut hits = 0;
    for _ in 0..3 {
        // Press again in the window after each swing ends.
        let mut guard = 0;
        while !(player.context == Context::Combo && player.anim.finished && !player.anim.blending()) && guard < 300 {
            hits += run(&mut player, &clips, Input::default(), 1).iter().filter(|e| matches!(e, Event::ComboHit { .. })).count();
            guard += 1;
        }
        hits += run(&mut player, &clips, Input::default(), 2).iter().filter(|e| matches!(e, Event::ComboHit { .. })).count();
        if player.combo_stage < 2 {
            run(&mut player, &clips, attack, 1);
        }
    }
    assert_eq!(player.combo_stage, 2);
    assert!(matches!(player.context_anim, Action::Combo(2, 5 | 6)), "{:?}", player.context_anim);
    hits += run(&mut player, &clips, Input::default(), 120).iter().filter(|e| matches!(e, Event::ComboHit { .. })).count();
    assert_eq!(hits, 3);
    assert_eq!(player.context, Context::None);

    // The next combo opens with the other chain; without a follow-up press it simply ends.
    run(&mut player, &clips, attack, 1);
    assert_eq!(player.context_anim, Action::Combo(1, 0));
    run(&mut player, &clips, Input::default(), 120);
    assert_eq!(player.context, Context::None);

    let events = run(&mut player, &clips, Input { special_pressed: true, ..Default::default() }, 1);
    assert!(events.contains(&Event::SaberOff));
    run(&mut player, &clips, Input::default(), 90);
    assert!(!player.weapon_out && player.blade_scale == 0.0);
}

#[test]
fn camera_follows_and_settles() {
    use crate::camera::{GameCamera, Rail};
    let (clips, mut player) = (clips(), Player::new(stats()));
    let mut camera = GameCamera::new(Rail::default());
    camera.update(player.pos, 0.42, DT);
    let start = camera.pos;
    for _ in 0..180 {
        player.step(&Input { stick_x: 1.0, ..Default::default() }, camera.yaw, &clips, DT);
        camera.update(player.pos, 0.42, DT);
    }
    // While the player runs the camera trails by speed / seek = 1.2 / 5 = 0.24 units.
    let lag = player.pos.x - (camera.pos.x - start.x);
    assert!((lag - 0.24).abs() < 0.03, "lag {lag}");
    for _ in 0..240 {
        player.step(&Input::default(), camera.yaw, &clips, DT);
        camera.update(player.pos, 0.42, DT);
    }
    assert!((camera.pos.x - player.pos.x).abs() < 0.01);
    assert!(camera.yaw_error().abs() < 64);
}

/// A box from (-1.5, 0, 2) to (1.5, 1.5, 5) on a floor, as collision triangles.
fn box_world() -> crate::world::World {
    let quad = |c: [[f32; 3]; 4], n: [f32; 3]| [([c[0], c[1], c[2]], n), ([c[0], c[2], c[3]], n)];
    let (x0, x1, y1, z0, z1) = (-1.5, 1.5, 1.5, 2.0, 5.0);
    let mut tris = Vec::new();
    tris.extend(quad([[-20.0, 0.0, -20.0], [20.0, 0.0, -20.0], [20.0, 0.0, 20.0], [-20.0, 0.0, 20.0]], [0.0, 1.0, 0.0]));
    tris.extend(quad([[x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1]], [0.0, 1.0, 0.0]));
    tris.extend(quad([[x0, 0.0, z0], [x1, 0.0, z0], [x1, y1, z0], [x0, y1, z0]], [0.0, 0.0, -1.0]));
    tris.extend(quad([[x0, 0.0, z1], [x1, 0.0, z1], [x1, y1, z1], [x0, y1, z1]], [0.0, 0.0, 1.0]));
    tris.extend(quad([[x0, 0.0, z0], [x0, 0.0, z1], [x0, y1, z1], [x0, y1, z0]], [-1.0, 0.0, 0.0]));
    tris.extend(quad([[x1, 0.0, z0], [x1, 0.0, z1], [x1, y1, z1], [x1, y1, z0]], [1.0, 0.0, 0.0]));
    crate::world::World::new(tris)
}

#[test]
fn stand_in_camera_rises_over_scenery_instead_of_entering_it() {
    use crate::camera::{GameCamera, Rail};
    let world = box_world();
    let mut camera = GameCamera::new(Rail::default());
    // In the open the goal is the plain offset: middle (0.21) + 0.98.
    let (open, _) = camera.stand_in_goal(crate::Vec3::new(0.0, 0.0, -5.0), 0.42, &world);
    assert!((open.y - 1.19).abs() < 1e-4, "{open:?}");
    // Standing just behind the box puts the plain offset inside it (z 3.3, y 1.19 < 1.5).
    let feet = crate::Vec3::new(0.0, 0.0, 5.5);
    let (goal, seek) = camera.stand_in_goal(feet, 0.42, &world);
    assert!((goal.y - 2.0).abs() < 1e-4, "box top 1.5 + 0.5 clearance, got {goal:?}");
    for _ in 0..240 {
        camera.update_towards(feet, 0.42, Some((goal, seek)), DT);
    }
    assert!(camera.pos.y > 1.9 && camera.pos.z > 2.0 && camera.pos.z < 5.0);
    // Nothing is above the settled camera.
    let high = crate::Vec3::new(camera.pos.x, camera.pos.y + 10.0, camera.pos.z);
    assert!(world.segment_hit(high, camera.pos).is_none());
}

#[test]
fn player_behind_scenery_counts_as_hidden_after_a_third_of_a_second() {
    use crate::camera::HiddenCheck;
    let world = box_world();
    let (camera, middle) = (crate::Vec3::new(0.0, 1.19, 0.0), crate::Vec3::new(0.0, 0.21, 5.5));
    let mut check = HiddenCheck::default();
    let mut frames = 0;
    while !check.hidden && frames < 120 {
        check.update(camera, middle, 0.1, 0.21, &world, DT);
        frames += 1;
    }
    let seconds = frames as f32 * DT;
    assert!(check.hidden && (0.28..0.4).contains(&seconds), "hidden after {seconds}");
    // In plain view it clears again, and the fade follows at 3 a second.
    let open = crate::Vec3::new(4.0, 0.21, 0.0);
    for _ in 0..60 {
        check.update(camera, open, 0.1, 0.21, &world, DT);
    }
    assert!(!check.hidden && check.fade == 0.0);
}

fn combat_step(combat: &mut crate::combat::Combat, player: &mut Player, clips: &Clips, mut input: Input) -> Vec<crate::combat::CombatEvent> {
    let middle = crate::Vec3::new(player.pos.x, player.pos.y + 0.21, player.pos.z);
    combat.events.clear();
    input.incoming_bolt = combat.incoming(middle, 0.21);
    player.step(&input, 0, clips, DT);
    combat.step(player, clips, clips, None, DT);
    combat.events.clone()
}

#[test]
fn unblocked_bolt_costs_a_heart() {
    use crate::combat::{Combat, CombatEvent};
    let (clips, mut player, mut combat) = (clips(), Player::new(stats()), Combat::default());
    combat.add_enemy(crate::Vec3::new(0.0, 0.0, 3.0), 0x8000);
    let mut events = Vec::new();
    for _ in 0..(4.0 / DT) as usize {
        events.extend(combat_step(&mut combat, &mut player, &clips, Input::default()));
        if events.contains(&CombatEvent::PlayerHit) {
            break;
        }
    }
    assert!(events.iter().any(|e| matches!(e, CombatEvent::Shot(_))), "the enemy fired");
    assert!(events.contains(&CombatEvent::PlayerHit));
    assert_eq!(player.hit_points, 3);
    // Knocked back along the bolt's path (it came from +z).
    assert!(player.vel.z < 0.0 || player.pos.z < 0.0, "{:?} {:?}", player.vel, player.pos);
}

#[test]
fn timed_block_returns_the_bolt_to_the_shooter() {
    use crate::combat::{Combat, CombatEvent};
    let (clips, mut player, mut combat) = (clips(), Player::new(stats()), Combat::default());
    player.weapon_out = true;
    player.blade_scale = 1.0;
    combat.add_enemy(crate::Vec3::new(0.0, 0.0, 3.0), 0x8000);
    let (mut events, mut pressed) = (Vec::new(), false);
    for _ in 0..(6.0 / DT) as usize {
        let middle = crate::Vec3::new(player.pos.x, player.pos.y + 0.21, player.pos.z);
        // Press attack on the first frame a bolt counts as incoming.
        let press = combat.incoming(middle, 0.21).is_some() && !pressed;
        pressed |= press;
        events.extend(combat_step(&mut combat, &mut player, &clips, Input { action_pressed: press, action_held: press, ..Default::default() }));
        if events.contains(&CombatEvent::EnemyDestroyed(0)) {
            break;
        }
    }
    assert!(events.iter().any(|e| matches!(e, CombatEvent::Deflected(_))), "{events:?}");
    assert!(events.contains(&CombatEvent::EnemyDestroyed(0)), "the returned bolt reached the shooter");
    assert_eq!(player.hit_points, 4);
}

#[test]
fn saber_destroys_a_droid_in_reach_only() {
    use crate::combat::{Combat, CombatEvent};
    let (player, mut combat) = (Player::new(stats()), Combat::default());
    combat.add_enemy(crate::Vec3::new(0.0, 0.0, 0.5), 0x8000);
    combat.add_enemy(crate::Vec3::new(0.0, 0.0, 2.0), 0x8000);
    combat.add_enemy(crate::Vec3::new(0.0, 0.0, -0.5), 0);
    combat.saber_hit(&player);
    assert_eq!(combat.events, vec![CombatEvent::EnemyDestroyed(0)], "only the one in front and close");
}
