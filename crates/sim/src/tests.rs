use super::*;

fn bot_inputs(sim: &Sim) -> Vec<PlayerInput> {
    (0..sim.players.len()).map(|i| bot::input_for(sim, i)).collect()
}

/// Steps with bots until `ticks` have passed, collecting every event.
fn run_bots(sim: &mut Sim, ticks: u32) -> Vec<Event> {
    let mut events = Vec::new();
    for _ in 0..ticks {
        let inputs = bot_inputs(sim);
        events.extend(sim.step(&inputs));
    }
    events
}

fn idle(sim: &Sim) -> Vec<PlayerInput> {
    vec![PlayerInput::default(); sim.players.len()]
}

fn count(events: &[Event], matches: impl Fn(&Event) -> bool) -> usize {
    events.iter().filter(|e| matches(e)).count()
}

#[test]
fn flight_reaches_its_target() {
    let origin = Vec3::new(-8.0, 2.0, 1.0);
    let target = Vec3::new(4.0, BALL_RADIUS, -2.0);
    let flight = Flight::to_target(origin, target, 1.3, 0);
    assert!(flight.position_at_time(1.3).distance(target) < 1e-4);
    assert!((flight.landing_time() - 1.3).abs() < 1e-4);
}

#[test]
fn serve_that_lands_in_scores_for_the_server() {
    let mut sim = Sim::new(MatchConfig::default());
    let Ball::Held { by: server } = sim.ball else { panic!("someone serves first") };
    let mut inputs = idle(&sim);
    inputs[server].pass = true;
    sim.step(&inputs);
    assert!(matches!(sim.ball, Ball::InFlight(_)));

    let mut events = Vec::new();
    for _ in 0..3 * TICK_HZ {
        events.extend(sim.step(&idle(&sim)));
    }
    assert!(events.contains(&Event::Point { team: 0, reason: PointReason::LandedIn }));
    assert_eq!(sim.score, [1, 0]);
}

#[test]
fn low_ball_hits_the_net_and_drops_back() {
    let mut sim = Sim::new(MatchConfig::default());
    let flight = Flight::to_target(Vec3::new(-3.0, 1.0, 0.0), Vec3::new(3.0, BALL_RADIUS, 0.0), 0.6, sim.tick);
    sim.ball = Ball::InFlight(flight);
    sim.touches = Touches { count: 1, last: Some(0), ..Touches::new(0) };

    let mut events = Vec::new();
    for _ in 0..2 * TICK_HZ {
        events.extend(sim.step(&idle(&sim)));
    }
    assert!(events.iter().any(|e| matches!(e, Event::HitNet { .. })));
    assert!(events.contains(&Event::Point { team: 1, reason: PointReason::LandedIn }));
}

#[test]
fn spike_from_a_set_clears_the_net() {
    // Roughly where a jumping player meets a set.
    let contact = Vec3::new(-1.8, 3.1, 0.0);
    let target = over_net_target(-1.0, None, SPIKE_DEPTH);
    let seconds = flight_seconds(HitKind::Spike, contact.with_y(0.0).distance(target.with_y(0.0)));
    let flight = Flight::to_target(contact, target, seconds, 0);
    let crossing = flight.position_at_time(flight.net_crossing_time().unwrap());
    assert!(crossing.y > NET_HEIGHT + BALL_RADIUS, "crossed at {}", crossing.y);
}

#[test]
fn aimed_hits_land_where_aimed() {
    let mut sim = Sim::new(MatchConfig::default());
    let hitter = sim.player_index(0, 0);
    let from = Vec3::new(-2.0, 3.0, 0.0);
    // Not holding it to serve.
    sim.ball = Ball::Dead { at: from };

    let spot = Vec2::new(7.0, -4.0);
    // Jumping with the ball right where a spike meets it best.
    sim.players[hitter].position = Vec3::new(-2.3, 0.9, 0.0);
    sim.players[hitter].aim = Some(spot);
    let plan = sim.plan_hit(hitter, MoveId::Spike, 3, from);
    assert_eq!(plan.preview.kind, HitKind::Spike);
    assert_eq!(plan.preview.spread, 0.0);
    sim.hit(hitter, plan, from, &mut Vec::new());
    assert!(landing(&sim).distance(spot) < 1e-3);

    let teammate_spot = Vec2::new(-2.0, 3.5);
    sim.players[hitter].position.y = 0.0;
    sim.players[hitter].aim = Some(teammate_spot);
    let plan = sim.plan_hit(hitter, MoveId::Pass, 1, from);
    assert_eq!(plan.preview.kind, HitKind::Pass);
    sim.hit(hitter, plan, from, &mut Vec::new());
    assert!(landing(&sim).distance(teammate_spot) < 1e-3);
}

fn landing(sim: &Sim) -> Vec2 {
    let at = sim.landing_point().expect("ball in flight");
    Vec2::new(at.x, at.z)
}

fn off_the_walls(at: Vec3) -> bool {
    at.x.abs() <= HALF_LENGTH - WALL_MARGIN && at.z.abs() <= HALF_WIDTH - WALL_MARGIN
}

#[test]
fn passes_stay_on_your_side() {
    let target = own_side_target(-1.0, Some(Vec2::new(8.0, 40.0)), SET_DEPTH);
    assert!(target.x < 0.0 && off_the_walls(target), "{target}");
}

#[test]
fn aims_stay_off_the_walls() {
    let target = over_net_target(-1.0, Some(Vec2::new(HALF_LENGTH + 5.0, -30.0)), OVER_DEPTH);
    assert!(target.x > 0.0 && off_the_walls(target), "{target}");
}

#[test]
fn walls_bounce_the_ball_back_in() {
    // Hit hard at the side wall: it comes back off it, still flying.
    let flight = Flight::to_target(Vec3::new(-5.0, 2.0, 8.0), Vec3::new(-5.0, BALL_RADIUS, 20.0), 1.0, 0);
    let landed = flight.landing_point();
    assert!(landed.z.abs() < HALF_WIDTH, "landed at {landed}");
    assert!((landed.z - (2.0 * (HALF_WIDTH - BALL_RADIUS) - 20.0)).abs() < 0.01, "mirrored off the wall: {landed}");
    assert_eq!(flight.wall_bounces(1.0), 1);
    assert!(flight.velocity_at_time(1.0).z < 0.0, "heading back from the wall");

    // In the arena: the bounce is an event, and the ball still scores where it lands.
    let mut sim = Sim::new(MatchConfig::default());
    sim.ball = Ball::InFlight(Flight { start_tick: sim.tick, ..flight });
    let mut events = Vec::new();
    for _ in 0..2 * TICK_HZ {
        events.extend(sim.step(&idle(&sim)));
    }
    assert!(events.iter().any(|e| matches!(e, Event::WallBounce { .. })), "{events:?}");
    assert!(events.contains(&Event::Point { team: 1, reason: PointReason::LandedIn }), "{events:?}");
}

#[test]
fn a_ball_off_the_end_wall_can_come_back_over_the_net() {
    // Hit flat and fast toward the far end wall, low enough to rebound into the net.
    let flight = Flight { origin: Vec3::new(20.0, 2.2, 0.0), velocity: Vec3::new(40.0, 1.5, 0.0), start_tick: 0 };
    let back = flight.next_net_crossing(0.0).expect("crosses back");
    assert!(back > (HALF_LENGTH - 20.0) / 40.0, "only after the wall: {back}");
    assert!(flight.position_at_time(back).x.abs() < 1e-3);
    assert!(flight.velocity_at_time(back).x < 0.0, "heading back toward the net");
}

/// A spike from team 0 straight at the net, with team 1's first player in the
/// air right behind it, `offset` meters to the side.
fn spike_into_block(offset: f32, kind: HitKind) -> (Sim, usize) {
    let mut sim = Sim::new(MatchConfig::default());
    let blocker = sim.player_index(1, 0);
    sim.players[blocker].position = Vec3::new(0.6, 1.2, offset);
    sim.players[blocker].hands_up = true;
    let from = Vec3::new(-1.5, 3.1, 0.0);
    let flight = Flight::to_target(from, Vec3::new(7.2, BALL_RADIUS, 0.0), 0.47, sim.tick);
    sim.ball = Ball::InFlight(flight);
    sim.touches = Touches { count: 3, last: Some(0), ..Touches::new(0) };
    sim.last_hit = Some(kind);
    (sim, blocker)
}

fn run_until_point(sim: &mut Sim) -> Vec<Event> {
    let mut events = Vec::new();
    while !events.iter().any(|e| matches!(e, Event::Point { .. })) && events.len() < 50 {
        let inputs = idle(sim);
        events.extend(sim.step(&inputs));
        assert!(sim.tick < 1000, "no point: {events:?}");
    }
    events
}

#[test]
fn square_block_stuffs_the_spike() {
    let (mut sim, blocker) = spike_into_block(0.0, HitKind::Spike);
    let events = run_until_point(&mut sim);
    assert!(events.contains(&Event::Blocked { player: blocker, stuffed: true }), "{events:?}");
    assert!(events.contains(&Event::Point { team: 1, reason: PointReason::LandedIn }), "{events:?}");
}

#[test]
fn edge_of_block_softens_the_spike() {
    let (mut sim, blocker) = spike_into_block(0.5, HitKind::Spike);
    let events = run_until_point(&mut sim);
    assert!(events.contains(&Event::Blocked { player: blocker, stuffed: false }), "{events:?}");
    // Pops up on the blockers' side; nobody plays it, so it drops there.
    assert!(events.contains(&Event::Point { team: 0, reason: PointReason::LandedIn }), "{events:?}");
}

#[test]
fn jumping_without_pressing_block_does_not_block() {
    let (mut sim, blocker) = spike_into_block(0.0, HitKind::Spike);
    sim.players[blocker].hands_up = false;
    let events = run_until_point(&mut sim);
    assert!(!events.iter().any(|e| matches!(e, Event::Blocked { .. })), "{events:?}");
}

#[test]
fn pass_at_the_net_jumps_to_block_only_when_the_ball_is_across() {
    let mut sim = Sim::new(MatchConfig::default());
    let player = sim.player_index(1, 0);
    sim.players[player].position = Vec3::new(0.6, 0.0, 0.0);
    // Ball on the other side: pass means block.
    sim.ball = Ball::Dead { at: Vec3::new(-3.0, 1.0, 0.0) };
    let mut inputs = idle(&sim);
    inputs[player].pass = true;
    sim.step(&inputs);
    assert!(sim.players[player].blocking(), "should jump with hands up");

    // Ball on our side: pass is a pass, no jump.
    let mut sim = Sim::new(MatchConfig::default());
    sim.players[player].position = Vec3::new(0.6, 0.0, 0.0);
    sim.ball = Ball::Dead { at: Vec3::new(2.0, 1.0, 0.0) };
    let mut inputs = idle(&sim);
    inputs[player].pass = true;
    sim.step(&inputs);
    assert!(sim.players[player].grounded() && !sim.players[player].blocking());
}

#[test]
fn serves_cannot_be_blocked() {
    let (mut sim, _) = spike_into_block(0.0, HitKind::Serve);
    let events = run_until_point(&mut sim);
    assert!(!events.iter().any(|e| matches!(e, Event::Blocked { .. })), "{events:?}");
}

#[test]
fn diving_digs_a_ball_out_of_running_reach() {
    let mut sim = Sim::new(MatchConfig::default());
    let digger = sim.player_index(1, 0);
    let start = sim.players[digger].position;
    // Comes down 3 m to the side, too far to run in the time left.
    let landing = start + Vec3::new(0.0, BALL_RADIUS, 3.0);
    sim.ball = Ball::InFlight(Flight::to_target(landing + Vec3::new(-6.0, 3.0, 0.0), landing, 1.0, sim.tick));
    sim.touches = Touches { count: 1, last: Some(0), ..Touches::new(0) };

    // Wait until the ball is about to land, then dive toward it.
    let mut events = Vec::new();
    while events.is_empty() {
        let Ball::InFlight(flight) = sim.ball else { break };
        let mut inputs = idle(&sim);
        if flight.landing_time() - flight.elapsed(sim.tick) < 0.3 {
            inputs[digger] = PlayerInput { dive: true, movement: Vec2::new(0.0, 1.0), ..default() };
        }
        events.extend(sim.step(&inputs));
    }
    for _ in 0..30 {
        events.extend(sim.step(&idle(&sim)));
    }
    assert!(events.contains(&Event::MoveStarted { player: digger, id: MoveId::Dive }), "{events:?}");
    assert!(events.contains(&Event::Touched { player: digger, kind: HitKind::Dig, quality: 1.0 }), "{events:?}");
}

/// A ball dropping `sideways` meters to the side of team 1's first player,
/// with `height` left before it lands.
fn low_ball_beside(sideways: f32) -> (Sim, usize) {
    let mut sim = Sim::new(MatchConfig::default());
    let player = sim.player_index(1, 0);
    let landing = sim.players[player].position + Vec3::new(0.0, BALL_RADIUS, sideways);
    sim.ball = Ball::InFlight(Flight::to_target(landing + Vec3::new(-6.0, 3.0, 0.0), landing, 1.0, sim.tick));
    sim.touches = Touches { count: 1, last: Some(0), ..Touches::new(0) };
    (sim, player)
}

/// Steps until the ball lands or is touched, pressing `input` for `player`
/// once the ball is within `when` seconds of landing.
fn play_ball(sim: &mut Sim, player: usize, when: f32, input: PlayerInput) -> Vec<Event> {
    let mut events = Vec::new();
    let mut pressed = false;
    while let Ball::InFlight(flight) = sim.ball {
        let mut inputs = idle(sim);
        if !pressed && flight.landing_time() - flight.elapsed(sim.tick) < when {
            inputs[player] = input;
            pressed = true;
        }
        events.extend(sim.step(&inputs));
        if events.iter().any(|e| matches!(e, Event::Touched { .. } | Event::Landed { .. })) {
            break;
        }
    }
    events
}

#[test]
fn foot_save_kicks_up_a_low_ball_out_of_arms_reach() {
    let (mut sim, player) = low_ball_beside(1.6);
    let kick = PlayerInput { kick: true, movement: Vec2::new(0.0, 1.0), ..default() };
    let events = play_ball(&mut sim, player, 0.15, kick);
    assert!(events.contains(&Event::MoveStarted { player, id: MoveId::FootSave }), "{events:?}");
    assert!(events.contains(&Event::Touched { player, kind: HitKind::Kick, quality: 1.0 }), "{events:?}");
    // It pops up for a teammate, on our side.
    let Ball::InFlight(flight) = sim.ball else { panic!("ball should be in flight") };
    assert_eq!(sim.team_on(flight.landing_point().x), 1);

    // A pass couldn't have reached it.
    let (mut sim, player) = low_ball_beside(1.6);
    let pass = PlayerInput { pass: true, ..default() };
    let events = play_ball(&mut sim, player, 0.15, pass);
    assert!(!events.iter().any(|e| matches!(e, Event::Touched { .. })), "{events:?}");
}

#[test]
fn foot_save_only_reaches_low_balls() {
    let sim = Sim::new(MatchConfig::default());
    let player = sim.player_index(1, 0);
    let at = sim.players[player].position;
    assert!(sim.players[player].reaches(MoveId::FootSave, at + Vec3::new(0.0, 0.3, 1.7)));
    assert!(!sim.players[player].reaches(MoveId::FootSave, at + Vec3::new(0.0, 1.5, 1.0)));
}

#[test]
fn only_feet_reach_a_ball_at_the_ankles() {
    let sim = Sim::new(MatchConfig::default());
    let player = &sim.players[sim.player_index(1, 0)];
    let at_the_ankles = player.position + Vec3::new(0.0, 0.3, 0.6);
    assert!(!player.reaches(MoveId::Pass, at_the_ankles));
    assert!(player.reaches(MoveId::FootSave, at_the_ankles));
}

#[test]
fn foot_save_leaves_you_stumbling() {
    let mut sim = Sim::new(MatchConfig::default());
    let player = sim.player_index(1, 0);
    let mut inputs = idle(&sim);
    inputs[player].kick = true;
    sim.step(&inputs);
    let spec = MoveId::FootSave.spec();
    for _ in 0..spec.windup + spec.active + 1 {
        sim.step(&idle(&sim));
    }
    let before = sim.players[player].position;
    let mut inputs = idle(&sim);
    inputs[player].movement = Vec2::new(1.0, 0.0);
    for _ in 0..5 {
        sim.step(&inputs);
    }
    assert_eq!(sim.players[player].position, before, "can't run while recovering");
}

#[test]
fn cannot_dive_in_the_air() {
    let mut sim = Sim::new(MatchConfig::default());
    let player = sim.player_index(1, 0);
    let mut inputs = idle(&sim);
    inputs[player].jump = true;
    sim.step(&inputs);
    let mut inputs = idle(&sim);
    inputs[player].dive = true;
    let events = sim.step(&inputs);
    assert!(!events.contains(&Event::MoveStarted { player, id: MoveId::Dive }));
}

#[test]
fn bots_play_real_rallies() {
    let mut sim = Sim::new(MatchConfig::default());
    let mut last_team = None;
    let mut unforced_errors = 0;
    let mut events = Vec::new();
    for _ in 0..300 * TICK_HZ {
        let inputs = bot_inputs(&sim);
        for event in sim.step(&inputs) {
            match event {
                Event::Touched { player, .. } | Event::Blocked { player, .. } => {
                    last_team = Some(sim.players[player].team);
                }
                // Dropped on their own side after touching it: a fumble, not a point won by the other team.
                Event::Landed { at, .. } if Some(sim.team_on(at.x)) == last_team => {
                    unforced_errors += 1;
                }
                _ => {}
            }
            events.push(event);
        }
    }
    let spikes = count(&events, |e| matches!(e, Event::Touched { kind: HitKind::Spike, .. }));
    let points = count(&events, |e| matches!(e, Event::Point { .. }));
    let faults = count(&events, |e| {
        matches!(e, Event::Point { reason: PointReason::DoubleTouch | PointReason::TooManyTouches, .. })
    });
    // One point every 5 to 20 seconds: rallies end, but not instantly.
    assert!((15..=60).contains(&points), "{points} points in 5 minutes");
    // Spikes win some points and get dug on others.
    let spikes_per_point = spikes as f32 / points as f32;
    assert!((1.2..=4.0).contains(&spikes_per_point), "{spikes_per_point:.2} spikes per point");
    assert!(unforced_errors * 10 <= points, "{unforced_errors} fumbles in {points} points");
    assert_eq!(faults, 0, "bots broke touch rules");
}

#[test]
fn one_and_three_a_side_bots_play() {
    for players_per_team in [1, 3] {
        let mut sim = Sim::new(MatchConfig::with_players(players_per_team));
        let events = run_bots(&mut sim, 60 * TICK_HZ);
        let touches = count(&events, |e| matches!(e, Event::Touched { .. }));
        assert!(touches >= 20, "{players_per_team}v{players_per_team}: only {touches} touches");
        assert!(!events.iter().any(|e| matches!(e, Event::Point { reason: PointReason::DoubleTouch, .. })));
    }
}

#[test]
fn same_inputs_give_same_match() {
    let mut a = Sim::new(MatchConfig::default());
    let mut b = a.clone();
    run_bots(&mut a, 20 * TICK_HZ);
    run_bots(&mut b, 20 * TICK_HZ);
    assert_eq!(a, b);
}

fn default<T: Default>() -> T {
    T::default()
}

/// Gives `team` the next point without playing it out.
fn give_point(sim: &mut Sim, team: usize) -> Vec<Event> {
    let mut events = Vec::new();
    sim.award_point(team, PointReason::LandedIn, Vec3::ZERO, &mut events);
    if matches!(sim.phase, Phase::PointScored { .. }) {
        sim.start_rally();
    }
    events
}

#[test]
fn a_set_goes_to_21_won_by_two() {
    let mut sim = Sim::new(MatchConfig::default());
    for _ in 0..20 {
        give_point(&mut sim, 0);
        give_point(&mut sim, 1);
    }
    assert_eq!(sim.score, [20, 20]);
    give_point(&mut sim, 0);
    assert_eq!((sim.score, sim.sets), ([21, 20], [0, 0]), "21-20 isn't a win");
    let events = give_point(&mut sim, 0);
    assert!(events.contains(&Event::SetWon { team: 0 }));
    assert_eq!((sim.score, sim.sets, sim.set), ([0, 0], [1, 0], 2));
}

#[test]
fn teams_switch_sides_every_seven_points() {
    let mut sim = Sim::new(MatchConfig::default());
    let red_side = sim.side(0);
    for point in 1..=7 {
        let events = give_point(&mut sim, point % 2);
        assert_eq!(events.contains(&Event::SidesSwitched), point == 7, "after point {point}");
    }
    assert_eq!(sim.side(0), -red_side);
    assert_eq!(sim.side(1), red_side);
    // Players line up on their team's new side, and play still works.
    let red = sim.player_index(0, 0);
    assert_eq!(court::half_of(sim.players[red].position.x), sim.side(0));
    let mut sim = sim;
    let events = run_bots(&mut sim, 20 * TICK_HZ);
    assert!(events.iter().any(|e| matches!(e, Event::Touched { kind: HitKind::Pass, .. })));
}

#[test]
fn two_sets_win_the_match_and_the_decider_goes_to_15() {
    let mut sim = Sim::new(MatchConfig::default());
    for _ in 0..21 {
        give_point(&mut sim, 0);
    }
    for _ in 0..21 {
        give_point(&mut sim, 1);
    }
    assert!(sim.deciding_set());
    assert_eq!(sim.points_to_win_set(), 15);
    let mut events = Vec::new();
    for _ in 0..15 {
        events = give_point(&mut sim, 1);
    }
    assert!(events.contains(&Event::MatchWon { team: 1 }));
    assert_eq!(sim.phase, Phase::MatchOver { winner: 1 });
    // Nothing moves once it's over.
    let before = sim.clone();
    let inputs = bot_inputs(&sim);
    sim.step(&inputs);
    assert_eq!(sim.players, before.players);
}


/// Team 0's first player jumping at `at` with a spike armed, and the ball held
/// still at `ball`, on team 0's side.
fn armed_jump(at: Vec3, ball: Vec3) -> (Sim, usize) {
    let mut sim = Sim::new(MatchConfig::default());
    let player = sim.player_index(0, 0);
    sim.players[player].position = at;
    sim.ball = Ball::Dead { at: ball };
    let mut inputs = idle(&sim);
    inputs[player].jump = true;
    sim.step(&inputs);
    let mut inputs = idle(&sim);
    inputs[player].spike = true;
    sim.step(&inputs);
    (sim, player)
}

#[test]
fn spike_stays_armed_until_landing() {
    let (mut sim, player) = armed_jump(Vec3::new(-6.0, 0.0, 0.0), Vec3::new(-2.0, 1.0, 0.0));
    let mut armed_ticks = 0;
    while !sim.players[player].grounded() {
        armed_ticks += u32::from(sim.players[player].active_move(sim.tick) == Some(MoveId::Spike));
        sim.step(&idle(&sim));
    }
    assert!(armed_ticks >= 35, "armed only {armed_ticks} ticks of the jump");
    assert_eq!(sim.players[player].action, None, "landing ends it");
}

#[test]
fn armed_spike_steers_toward_the_ball() {
    let ball = Vec3::new(-2.0, 3.2, 1.5);
    let (mut armed, player) = armed_jump(Vec3::new(-3.5, 0.0, 0.0), ball);
    for _ in 0..30 {
        armed.step(&idle(&armed));
    }
    let body = armed.players[player].position;
    let wanted = attack::steer_position(&armed.players[player], ball);
    assert!(Vec2::new(body.x, body.z).distance(wanted) < 0.3, "at {body}, wanted {wanted}");

    // A plain jump goes straight up.
    let mut plain = Sim::new(MatchConfig::default());
    plain.players[player].position = Vec3::new(-3.5, 0.0, 0.0);
    plain.ball = Ball::Dead { at: ball };
    let mut inputs = idle(&plain);
    inputs[player].jump = true;
    for _ in 0..31 {
        plain.step(&inputs);
        inputs = idle(&plain);
    }
    assert_eq!(plain.players[player].position.with_y(0.0), Vec3::new(-3.5, 0.0, 0.0));
}

#[test]
fn attacks_use_whatever_reaches_the_ball() {
    // Team 0 faces +x, toward the net.
    let body = Player::new(0, -1.0, Vec3::new(-3.0, 1.0, 0.0));
    let (kind, quality) = attack::best_technique(&body, body.position + Vec3::new(0.3, 2.1, 0.0));
    assert_eq!((kind, quality), (HitKind::Spike, 1.0));
    let (kind, _) = attack::best_technique(&body, body.position + Vec3::new(0.6, 0.8, 0.0));
    assert_eq!(kind, HitKind::Volley, "low in front");
    let (kind, _) = attack::best_technique(&body, body.position + Vec3::new(-0.6, 1.8, 0.0));
    assert_eq!(kind, HitKind::Bicycle, "behind the head");
}

#[test]
fn off_position_attacks_are_weaker_and_wilder() {
    let mut sim = Sim::new(MatchConfig::default());
    let hitter = sim.player_index(0, 0);
    let from = Vec3::new(-2.0, 3.0, 0.0);
    sim.ball = Ball::Dead { at: from };
    sim.players[hitter].aim = Some(Vec2::new(6.0, 0.0));

    sim.players[hitter].position = Vec3::new(-2.3, 0.9, 0.0);
    let clean = sim.plan_hit(hitter, MoveId::Spike, 3, from);
    sim.players[hitter].position = Vec3::new(-2.3, 0.9, 0.9);
    let off = sim.plan_hit(hitter, MoveId::Spike, 3, from);

    assert_eq!(clean.preview.quality, 1.0);
    assert!(off.preview.quality < 0.6, "quality {}", off.preview.quality);
    assert!(off.seconds > clean.seconds * 1.2, "not slower: {} vs {}", off.seconds, clean.seconds);
    assert!(off.preview.spread > 1.0, "not wilder: {}", off.preview.spread);
}

#[test]
fn armed_spike_waits_for_the_sweet_spot() {
    let mut sim = Sim::new(MatchConfig::default());
    let player = sim.player_index(0, 0);
    sim.players[player].position = Vec3::new(-2.0, 0.0, 0.0);
    sim.touches = Touches { count: 2, last: Some(1), ..Touches::new(0) };
    // A set dropping onto the spot a spike meets best at the top of the jump.
    let apex = Vec3::new(-1.7, 1.225 + 2.1, 0.0);
    let rise = sim.players[player].kit.jump_speed / player::PLAYER_GRAVITY;
    let velocity = Vec3::new(-0.5, -3.0, 0.0);
    let origin = apex - velocity * rise + Vec3::Y * 0.5 * court::BALL_GRAVITY * rise * rise;
    sim.ball = Ball::InFlight(Flight { origin, velocity, start_tick: sim.tick });

    let mut inputs = idle(&sim);
    inputs[player].jump = true;
    inputs[player].spike = true;
    let mut contact = None;
    for _ in 0..TICK_HZ {
        let events = sim.step(&inputs);
        inputs = idle(&sim);
        inputs[player].spike = sim.players[player].active_move(sim.tick).is_none() && !sim.players[player].grounded();
        if let Some(&Event::Touched { kind, quality, .. }) = events.iter().find(|e| matches!(e, Event::Touched { .. })) {
            contact = Some((kind, quality));
            break;
        }
    }
    let (kind, quality) = contact.expect("the spike connects");
    assert_eq!(kind, HitKind::Spike);
    assert!(quality > 0.8, "hit at quality {quality}");
}

#[test]
fn ball_behind_the_head_gets_a_bicycle_kick() {
    let mut sim = Sim::new(MatchConfig::default());
    let player = sim.player_index(0, 0);
    let tick = sim.tick;
    // Rising with an attack armed, the ball hanging just behind.
    sim.players[player].position = Vec3::new(-4.0, 0.3, 0.0);
    sim.players[player].vertical_velocity = 5.0;
    sim.players[player].action = Some(Action { id: MoveId::Spike, start_tick: tick, direction: Vec2::X, spent: false });
    sim.ball = Ball::InFlight(Flight { origin: Vec3::new(-4.9, 3.0, 0.2), velocity: Vec3::ZERO, start_tick: tick });
    let events = run_bots_idle(&mut sim, TICK_HZ);
    assert!(events.iter().any(|e| matches!(e, Event::Touched { player: 0, kind: HitKind::Bicycle, .. })), "{events:?}");
}

fn run_bots_idle(sim: &mut Sim, ticks: u32) -> Vec<Event> {
    (0..ticks).flat_map(|_| sim.step(&idle(sim))).collect()
}


/// Team 0's first player, alone on an empty court with the ball out of play.
fn lone_player() -> (Sim, usize) {
    let mut sim = Sim::new(MatchConfig::default());
    let player = sim.player_index(0, 0);
    sim.players[player].position = Vec3::new(-6.0, 0.0, 0.0);
    sim.ball = Ball::Dead { at: Vec3::new(5.0, 0.1, 0.0) };
    (sim, player)
}

/// How high `player` gets if they jump now, given `inputs` for the rest of the jump.
fn jump_apex(sim: &mut Sim, player: usize, after: impl Fn(u32) -> PlayerInput) -> f32 {
    let mut input = PlayerInput { jump: true, ..after(0) };
    let mut apex: f32 = 0.0;
    for tick in 1..=TICK_HZ {
        let mut inputs = idle(sim);
        inputs[player] = input;
        sim.step(&inputs);
        apex = apex.max(sim.players[player].position.y);
        input = after(tick);
    }
    apex
}

#[test]
fn running_jumps_go_higher() {
    let (mut standing, player) = lone_player();
    let standing_apex = jump_apex(&mut standing, player, |_| PlayerInput::default());

    let (mut running, _) = lone_player();
    let run = PlayerInput { movement: Vec2::X, ..default() };
    for _ in 0..30 {
        let mut inputs = idle(&running);
        inputs[player] = run;
        running.step(&inputs);
    }
    let running_apex = jump_apex(&mut running, player, |_| run);
    assert!(running_apex > standing_apex * 1.25, "{running_apex} vs {standing_apex}");
}

#[test]
fn letting_go_early_is_a_short_hop() {
    let (mut full, player) = lone_player();
    let full_apex = jump_apex(&mut full, player, |_| PlayerInput::default());
    let (mut hop, _) = lone_player();
    let hop_apex = jump_apex(&mut hop, player, |tick| PlayerInput { jump_released: tick == 3, ..default() });
    assert!(hop_apex < full_apex * 0.75, "{hop_apex} vs {full_apex}");
    // Letting go on the way down changes nothing.
    let (mut late, _) = lone_player();
    let late_apex = jump_apex(&mut late, player, |tick| PlayerInput { jump_released: tick == 30, ..default() });
    assert_eq!(late_apex, full_apex);
}

#[test]
fn dash_bursts_then_cools_down() {
    let (mut sim, player) = lone_player();
    let dash = PlayerInput { dash: true, movement: Vec2::Y, ..default() };
    let mut inputs = idle(&sim);
    inputs[player] = dash;
    let events = sim.step(&inputs);
    assert!(events.contains(&Event::Dashed { player }));
    for _ in 0..player::DASH_TICKS {
        sim.step(&idle(&sim));
    }
    let covered = sim.players[player].position.z;
    assert!(covered > 1.8, "dashed only {covered} m");

    // Too soon for another.
    let mut inputs = idle(&sim);
    inputs[player] = dash;
    assert!(!sim.step(&inputs).contains(&Event::Dashed { player }));
    for _ in 0..TICK_HZ {
        sim.step(&idle(&sim));
    }
    let mut inputs = idle(&sim);
    inputs[player] = dash;
    assert!(sim.step(&inputs).contains(&Event::Dashed { player }), "ready again");
}

#[test]
fn fast_landings_skid() {
    /// How far the player slides after landing from a jump taken at `speed`
    /// (away from the net), letting go of the stick.
    fn slide(speed: f32) -> f32 {
        let (mut sim, player) = lone_player();
        let run = Vec2::new(-speed / 6.5, 0.0);
        sim.players[player].velocity = run * 6.5;
        let mut inputs = idle(&sim);
        inputs[player] = PlayerInput { jump: true, movement: run, ..default() };
        sim.step(&inputs);
        while !sim.players[player].grounded() {
            let mut inputs = idle(&sim);
            inputs[player].movement = run;
            sim.step(&inputs);
        }
        let landed = sim.players[player].position.x;
        for _ in 0..TICK_HZ {
            sim.step(&idle(&sim));
        }
        landed - sim.players[player].position.x
    }
    // Grip comes back after the skid, so it slides a bit farther than a normal stop.
    let normal_stop = 6.5 * 6.5 / (2.0 * 55.0);
    assert!(slide(6.5) > normal_stop * 1.4, "slid {}", slide(6.5));
    assert!(slide(2.0) < 2.0 * 2.0 / (2.0 * 55.0) + 0.05, "slow landings don't skid");
}


/// A match with Cross as team 0's first player, the ball dropping onto him
/// from `ball`, and his team on `touches` touches with `last` touching last.
fn cross_under_ball(ball: Vec3, touches: u32, last: Option<usize>) -> (Sim, usize) {
    let mut sim = Sim::new(MatchConfig::default());
    let cross = sim.player_index(0, 0);
    sim.set_kit(cross, moves::CROSS);
    sim.players[cross].position = Vec3::new(ball.x, 0.0, ball.z);
    sim.ball = Ball::InFlight(Flight { origin: ball, velocity: Vec3::ZERO, start_tick: sim.tick });
    sim.touches = Touches { count: touches, last, ..Touches::new(0) };
    (sim, cross)
}

/// Steps until the ball has been touched (or a second passes), pressing
/// `input` for `player` every tick.
fn play_until_touch(sim: &mut Sim, player: usize, input: PlayerInput) -> Vec<Event> {
    let mut events = Vec::new();
    for _ in 0..TICK_HZ {
        let mut inputs = idle(sim);
        inputs[player] = input;
        events.extend(sim.step(&inputs));
        if events.iter().any(|e| matches!(e, Event::Touched { .. } | Event::Point { .. })) {
            break;
        }
    }
    events
}

#[test]
fn a_dribbler_touches_twice_in_a_row_once_per_possession() {
    let (mut sim, cross) = cross_under_ball(Vec3::new(-4.0, 1.6, 0.0), 1, Some(0));
    let events = play_until_touch(&mut sim, cross, PlayerInput { pass: true, ..default() });
    assert!(events.contains(&Event::Dribbled { player: cross }), "{events:?}");
    assert!(events.iter().any(|e| matches!(e, Event::Touched { player: 0, .. })), "{events:?}");
    // Used up for this possession: a third touch in a row would be a fault.
    assert!(sim.must_not_touch(cross));

    // Anyone else doing it is a double touch.
    let (mut sim, player) = cross_under_ball(Vec3::new(-4.0, 1.6, 0.0), 1, Some(0));
    sim.set_kit(player, moves::ALL_ROUNDER);
    let events = play_until_touch(&mut sim, player, PlayerInput { pass: true, ..default() });
    assert!(events.contains(&Event::Point { team: 1, reason: PointReason::DoubleTouch }), "{events:?}");
}

#[test]
fn no_look_hits_are_read_late() {
    let mut sim = Sim::new(MatchConfig::default());
    let hitter = sim.player_index(0, 0);
    let defender = sim.player_index(1, 0);
    let flight = Flight::to_target(Vec3::new(-3.0, 2.0, 0.0), Vec3::new(6.0, BALL_RADIUS, 0.0), 1.2, sim.tick);
    sim.last_hitter = Some(hitter);
    let plain = bot::reaction_ticks(&sim, defender, &flight);
    sim.set_kit(hitter, moves::CROSS);
    let no_look = bot::reaction_ticks(&sim, defender, &flight);
    assert!(no_look > plain, "{no_look} vs {plain}");
    // Teammates read it fine.
    assert_eq!(bot::reaction_ticks(&sim, sim.player_index(0, 1), &flight), plain);
}

/// Cross in the air by the net with `input` pressed, a blocker up at the net
/// lined up with the ball. Returns the events until the point.
fn attack_into_block(input: PlayerInput) -> Vec<Event> {
    let ball = Vec3::new(-1.3, 3.4, 0.5);
    let (mut sim, cross) = cross_under_ball(ball, 2, Some(1));
    sim.players[cross].position = Vec3::new(-1.6, 0.6, 0.5);
    sim.players[cross].vertical_velocity = 3.0;
    sim.players[cross].aim = Some(Vec2::new(7.0, 0.0));
    let blocker = sim.player_index(1, 0);
    let mut events = Vec::new();
    let mut input = input;
    for _ in 0..2 * TICK_HZ {
        // Hold the blocker up at the net, hands high, lined up with the ball.
        let b = &mut sim.players[blocker];
        b.position = Vec3::new(0.6, 1.1, 0.5);
        b.vertical_velocity = 0.0;
        b.hands_up = true;
        let mut inputs = idle(&sim);
        inputs[cross] = PlayerInput { aim: Some(Vec2::new(7.0, 0.0)), ..input };
        input.ability = false;
        input.spike = false;
        events.extend(sim.step(&inputs));
        if events.iter().any(|e| matches!(e, Event::Point { .. })) {
            break;
        }
    }
    events
}

#[test]
fn crossover_carries_the_ball_around_the_block() {
    let spiked = attack_into_block(PlayerInput { spike: true, ..default() });
    assert!(spiked.iter().any(|e| matches!(e, Event::Blocked { .. })), "a plain spike is blocked: {spiked:?}");

    let crossed = attack_into_block(PlayerInput { ability: true, movement: Vec2::new(0.0, 1.0), ..default() });
    assert!(crossed.iter().any(|e| matches!(e, Event::Carried { player: 0 })), "{crossed:?}");
    assert!(crossed.iter().any(|e| matches!(e, Event::Touched { player: 0, kind: HitKind::Spike, .. })), "{crossed:?}");
    assert!(!crossed.iter().any(|e| matches!(e, Event::Blocked { .. })), "got blocked: {crossed:?}");
}

#[test]
fn abilities_wait_for_their_cooldown() {
    let (mut sim, player) = lone_player();
    sim.set_kit(player, moves::CROSS);
    let press = |sim: &mut Sim| {
        let mut inputs = idle(sim);
        inputs[player] = PlayerInput { jump: true, ..default() };
        sim.step(&inputs);
        let mut inputs = idle(sim);
        inputs[player] = PlayerInput { ability: true, ..default() };
        let started = sim.step(&inputs).contains(&Event::MoveStarted { player, id: MoveId::Crossover });
        while !sim.players[player].grounded() {
            sim.step(&idle(sim));
        }
        started
    };
    assert!(press(&mut sim));
    assert!(!press(&mut sim), "still cooling down");
    for _ in 0..7 * TICK_HZ {
        sim.step(&idle(&sim));
    }
    assert!(press(&mut sim), "ready again");
}

#[test]
fn posterizer_needs_a_full_charge_and_leaps_high() {
    let (mut sim, player) = lone_player();
    sim.set_kit(player, moves::CROSS);
    let mut inputs = idle(&sim);
    inputs[player] = PlayerInput { ultimate: true, ..default() };
    assert!(!sim.step(&inputs).contains(&Event::MoveStarted { player, id: MoveId::Posterizer }), "not charged");

    sim.players[player].charge = 1.0;
    assert!(sim.step(&inputs).contains(&Event::MoveStarted { player, id: MoveId::Posterizer }));
    assert_eq!(sim.players[player].charge, 0.0, "uses the charge");
    let mut apex: f32 = 0.0;
    let mut air_ticks = 0;
    while !sim.players[player].grounded() {
        sim.step(&idle(&sim));
        apex = apex.max(sim.players[player].position.y);
        air_ticks += 1;
    }
    let (mut normal, _) = lone_player();
    let normal_apex = jump_apex(&mut normal, player, |_| PlayerInput::default());
    assert!(apex > normal_apex * 1.8, "{apex} vs {normal_apex}");
    assert!(air_ticks > TICK_HZ, "hangs: {air_ticks} ticks");
}

#[test]
fn dunks_go_through_blocks_and_knock_blockers_down() {
    let (mut sim, blocker) = spike_into_block(0.0, HitKind::Dunk);
    let events = run_until_point(&mut sim);
    assert!(events.contains(&Event::Posterized { player: blocker }), "{events:?}");
    assert!(!events.iter().any(|e| matches!(e, Event::Blocked { .. })), "{events:?}");
    assert!(events.contains(&Event::Point { team: 0, reason: PointReason::LandedIn }), "{events:?}");
    assert!(sim.players[blocker].stunned(sim.tick) || sim.phase != Phase::Rally);
}

#[test]
fn heroes_with_kits_play_real_rallies() {
    let mut sim = Sim::new(MatchConfig::default());
    sim.set_kit(sim.player_index(0, 0), moves::CROSS);
    sim.set_kit(sim.player_index(1, 1), moves::CROSS);
    let events = run_bots(&mut sim, 300 * TICK_HZ);
    let points = count(&events, |e| matches!(e, Event::Point { .. }));
    assert!((15..=60).contains(&points), "{points} points");
    assert!(count(&events, |e| matches!(e, Event::Carried { .. })) > 0, "no crossovers");
    assert!(count(&events, |e| matches!(e, Event::Dribbled { .. })) > 0, "no dribbles");
    assert!(count(&events, |e| matches!(e, Event::Touched { kind: HitKind::Dunk, .. })) > 0, "no dunks");
    let faults = count(&events, |e| matches!(e, Event::Point { reason: PointReason::DoubleTouch | PointReason::TooManyTouches, .. }));
    assert_eq!(faults, 0);
}

