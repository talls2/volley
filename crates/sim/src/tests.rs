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
    sim.touches = Touches { team: 0, count: 1, last: Some(0) };

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
    let flight = Flight::to_target(contact, opponent_target(0, Vec2::ZERO, SPIKE_DEPTH), SPIKE_SECONDS, 0);
    let crossing = flight.position_at_time(flight.net_crossing_time().unwrap());
    assert!(crossing.y > NET_HEIGHT + BALL_RADIUS, "crossed at {}", crossing.y);
}

#[test]
fn diving_digs_a_ball_out_of_running_reach() {
    let mut sim = Sim::new(MatchConfig::default());
    let digger = sim.player_index(1, 0);
    let start = sim.players[digger].position;
    // Comes down 3 m to the side, too far to run in the time left.
    let landing = start + Vec3::new(0.0, BALL_RADIUS, 3.0);
    sim.ball = Ball::InFlight(Flight::to_target(landing + Vec3::new(-6.0, 3.0, 0.0), landing, 1.0, sim.tick));
    sim.touches = Touches { team: 0, count: 1, last: Some(0) };

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
    assert!(events.contains(&Event::Dove { player: digger }), "{events:?}");
    assert!(events.contains(&Event::Touched { player: digger, kind: HitKind::Dig }), "{events:?}");
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
    assert!(!events.contains(&Event::Dove { player }));
}

#[test]
fn bots_play_real_rallies() {
    let mut sim = Sim::new(MatchConfig::default());
    let events = run_bots(&mut sim, 90 * TICK_HZ);
    let returns = count(&events, |e| matches!(e, Event::Touched { kind: HitKind::Pass | HitKind::Dig, .. }));
    let spikes = count(&events, |e| matches!(e, Event::Touched { kind: HitKind::Spike, .. }));
    let points = count(&events, |e| matches!(e, Event::Point { .. }));
    let faults = count(&events, |e| {
        matches!(e, Event::Point { reason: PointReason::DoubleTouch | PointReason::TooManyTouches, .. })
    });
    assert!(returns >= 20, "only {returns} passes in 90 s");
    assert!(spikes >= 5, "only {spikes} spikes in 90 s");
    assert!(points >= 5, "only {points} points in 90 s");
    assert_eq!(faults, 0, "bots broke touch rules");
}

#[test]
fn one_and_three_a_side_bots_play() {
    for players_per_team in [1, 3] {
        let mut sim = Sim::new(MatchConfig { players_per_team });
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
