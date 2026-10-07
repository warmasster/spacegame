//! The game in steps of its own (`lunar_play::game`): what it ends up as does not depend on how
//! the time is cut into frames, nor on the machine or the process it runs in; it steps any number
//! of players; and what is drawn between steps never jumps.
use glam::{DVec3, Quat, Vec3};
use lunar_play::{
    defs::Defs,
    game::{Game, Player, STEP, digest},
    pilot::Input,
};
use std::sync::OnceLock;

fn defs() -> &'static Defs {
    static DEFS: OnceLock<Defs> = OnceLock::new();
    DEFS.get_or_init(|| Defs::load(&lunar_play::root().join("assets/defs")).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message)))
}

fn new_game() -> Game {
    Game::new(defs(), &lunar_play::root().join("assets/defs"), 4000, |_| true).unwrap()
}

fn new_player(game: &Game) -> Player {
    Player::new(game.bodies.clone(), &game.site, defs().scenario.player)
}

/// What the player asks at step `n` of the walk the tests take: ahead, a turn of the look, a jump,
/// a crouch, a dash to the side.
fn walk(n: u64) -> Input {
    let mut i = Input::default();
    if (30..400).contains(&n) {
        i.forward = 1.0;
    }
    if (200..260).contains(&n) {
        i.side = -1.0;
        i.run = true;
    }
    if n == 120 || n == 330 {
        i.jump = true;
    }
    if (280..320).contains(&n) {
        i.crouch = true;
    }
    i
}

/// The game run as a frame loop does: each frame owes it its time, taken in steps of `STEP` (as
/// many as fit, 4 at most), each with what the player asks at that step, until `steps` are taken.
fn run(frames: impl Iterator<Item = f64>, steps: u64) -> u64 {
    let mut game = new_game();
    let mut me = new_player(&game);
    let mut due = 0.0;
    for dt in frames {
        due += dt;
        let mut taken = 0;
        while due >= STEP && taken < 4 && game.step < steps {
            due -= STEP;
            taken += 1;
            me.input = walk(game.step);
            if game.step == 150 {
                me.pilot.look_by(0.6, -0.1);
            }
            game.tick(&mut [&mut me]);
        }
        if game.step >= steps {
            break;
        }
    }
    assert_eq!(game.step, steps);
    digest(&game, &[&me])
}

#[test]
fn the_game_is_the_same_however_the_frames_cut_it() {
    // 30, 60, 144 and 240 frames a second, and frames that come as they please (a stutter now
    // and then): the same steps, the same game
    let steps = 480;
    let even = |fps: f64| std::iter::repeat(1.0 / fps);
    let mut seed = 0x5eed_u64;
    let ragged = std::iter::from_fn(move || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let r = (seed >> 33) as f64 / f64::from(1u32 << 31);
        Some(if r > 0.95 { 0.05 } else { 0.004 + r * 0.02 })
    });
    let want = run(even(60.0), steps);
    for (name, got) in [("30 fps", run(even(30.0), steps)), ("144 fps", run(even(144.0), steps)), ("240 fps", run(even(240.0), steps)), ("a trompicones", run(ragged, steps))] {
        assert_eq!(got, want, "a {name} the game ends otherwise than at 60 fps");
    }
}

#[test]
fn the_same_steps_give_the_same_game_in_another_process() {
    // (whatever depends on the process — a map's order, a hash's seed — would show here)
    let steps = 360;
    let ours = run(std::iter::repeat(STEP), steps);
    if std::env::var("LUNAR_DIGEST_CHILD").is_ok() {
        println!("DIGEST={ours}");
        return;
    }
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["the_same_steps_give_the_same_game_in_another_process", "--exact", "--nocapture", "--test-threads", "1"])
        .env("LUNAR_DIGEST_CHILD", "1")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let theirs: u64 = text.lines().find_map(|l| l.split("DIGEST=").nth(1)).and_then(|d| d.split_whitespace().next()?.parse().ok()).unwrap_or_else(|| panic!("the other process said nothing: {text}"));
    assert_eq!(theirs, ours, "another process ends otherwise");
}

#[test]
fn every_player_is_stepped_alike() {
    // two players, one walking the walk and one standing: each goes as its keys say, the one
    // standing where it stood; and the walker goes as it would alone
    let mut game = new_game();
    let (mut ana, mut luis) = (new_player(&game), new_player(&game));
    // (Luis on the ground 6 m to one side, and both settled on it)
    let b = game.bodies.get(game.site.body);
    let (feet, up) = luis.pilot.feet();
    let side = up.any_orthonormal_vector();
    luis.pilot.put(b.above_ground(b.up(feet + side * 6.0), 0.0), b.up(feet + side * 6.0));
    for _ in 0..60 {
        game.tick(&mut [&mut ana, &mut luis]);
    }
    let (ana_from, luis_from) = (ana.pilot.position, luis.pilot.position);
    for n in 0..240 {
        ana.input = walk(n);
        game.tick(&mut [&mut ana, &mut luis]);
    }
    let (ana_to, luis_to) = (ana.pilot.position, luis.pilot.position);
    assert!(ana_to.distance(ana_from) > 3.0, "the walker went {:.2} m", ana_to.distance(ana_from));
    assert!(luis_to.distance(luis_from) < 0.05, "the one standing moved {:.3} m", luis_to.distance(luis_from));
    let mut alone = new_game();
    let mut solo = new_player(&alone);
    for _ in 0..60 {
        alone.tick(&mut [&mut solo]);
    }
    for n in 0..240 {
        solo.input = walk(n);
        alone.tick(&mut [&mut solo]);
    }
    assert!(solo.pilot.position.distance(ana_to) < 1e-6, "with another player there the walker ends {:.4} m off", solo.pilot.position.distance(ana_to));
}

#[test]
fn drawn_between_steps_nothing_jumps_at_any_speed() {
    // a ship far from every body (nothing pulls) going at 0, 300 and 7 800 m/s with the player
    // standing in it, drawn at 144 frames a second between its steps: the ship goes on as smooth
    // as it goes (each frame as far as its speed takes it in that frame) and the eye stays put in
    // it, however far a step takes it
    for speed in [0.0, 300.0, 7800.0] {
        let mut game = new_game();
        let mut me = new_player(&game);
        let far = DVec3::new(2.0e6, 3.0e6, -1.0e6);
        // (watched from there from the start: a structure nobody watches is stepped now and then,
        // and one put aboard it would be left behind when it catches up)
        game.watchers.push(far);
        let id = game.ships.spawn_free(&mut game.builds, "alcotan", far, Quat::IDENTITY).unwrap();
        let k = game.builds.set.index_of(id).unwrap();
        game.builds.set.list[k].vel = DVec3::new(speed, 0.0, 0.0);
        // (its systems give it its rooms and its weight once they run; then we stand where its
        // pilot gets up to, and settle)
        game.tick(&mut [&mut me]);
        let n = game.ships.by_structure(id).unwrap();
        let exit = Vec3::from_array(game.ships.list[n].kind.seats[0].def.salida);
        me.pilot.put_on(&game.builds.set, id, exit);
        for _ in 0..60 {
            game.tick(&mut [&mut me]);
        }
        assert!(me.pilot.ride.is_some_and(|r| r.id == id), "at {speed} m/s we are not aboard");
        let frame = 1.0 / 144.0;
        let (mut due, mut last): (f64, Option<(DVec3, Vec3)>) = (0.0, None);
        let (mut worst_ship, mut worst_eye) = (0.0f64, 0.0f32);
        for _ in 0..288 {
            due += frame;
            while due >= STEP {
                due -= STEP;
                game.tick(&mut [&mut me]);
            }
            game.present(due / STEP, &mut [&mut me]);
            let s = game.builds.set.get(id).unwrap();
            let eye = me.pilot.view_aboard(&game.builds.set).unwrap().eye;
            let now = (s.pos, s.to_local(eye));
            if let Some((pos, local)) = last {
                worst_ship = worst_ship.max(((now.0 - pos) - s.vel * frame).length());
                worst_eye = worst_eye.max((now.1 - local).length());
            }
            last = Some(now);
            game.restore(&mut [&mut me]);
        }
        assert!(worst_ship < 1e-3, "at {speed} m/s the ship drawn jumps {:.4} m off its way between frames", worst_ship);
        assert!(worst_eye < 1e-3, "at {speed} m/s the eye jumps {:.4} m in the ship between frames", worst_eye);
    }
}
