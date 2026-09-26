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
    let serve = PlayerInput { pass: true, ..default_input() };
    sim.step(&[serve, default_input()]);
    assert!(matches!(sim.ball, Ball::InFlight(_)));

    let mut events = Vec::new();
    for _ in 0..3 * TICK_HZ {
        events.extend(sim.step(&[default_input(), default_input()]));
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
        events.extend(sim.step(&[default_input(), default_input()]));
    }
    assert!(events.iter().any(|e| matches!(e, Event::HitNet { .. })));
    assert!(events.contains(&Event::Point { team: 1, reason: PointReason::LandedIn }));
}

#[test]
fn spike_from_a_set_clears_the_net() {
    // Roughly where a jumping player meets a default set.
    let contact = Vec3::new(-1.8, 3.2, 0.0);
    let flight = Flight::to_target(contact, opponent_target(0, Vec2::ZERO, SPIKE_DEPTH), SPIKE_SECONDS, 0);
    let crossing = flight.position_at_time(flight.net_crossing_time().unwrap());
    assert!(crossing.y > NET_HEIGHT + BALL_RADIUS, "crossed at {}", crossing.y);
}

#[test]
fn bots_keep_rallies_going() {
    let mut sim = Sim::new(MatchConfig::default());
    let events = run_bots(&mut sim, 30 * TICK_HZ);
    let returns = events
        .iter()
        .filter(|e| matches!(e, Event::Touched { kind: HitKind::Pass | HitKind::Lob, .. }))
        .count();
    assert!(returns >= 10, "only {returns} returns in 30 s");
}

#[test]
fn three_a_side_bots_play() {
    let mut sim = Sim::new(MatchConfig { players_per_team: 3 });
    let events = run_bots(&mut sim, 30 * TICK_HZ);
    assert!(events.iter().filter(|e| matches!(e, Event::Touched { .. })).count() >= 10);
    assert!(!events.iter().any(|e| matches!(e, Event::Point { reason: PointReason::DoubleTouch, .. })));
}

#[test]
fn same_inputs_give_same_match() {
    let mut a = Sim::new(MatchConfig::default());
    let mut b = a.clone();
    run_bots(&mut a, 20 * TICK_HZ);
    run_bots(&mut b, 20 * TICK_HZ);
    assert_eq!(a, b);
}

fn default_input() -> PlayerInput {
    PlayerInput::default()
}
