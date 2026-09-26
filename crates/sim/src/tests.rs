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
    let target = over_net_target(0, None, SPIKE_DEPTH);
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
    sim.players[hitter].position.y = 1.0;
    sim.players[hitter].aim = Some(spot);
    let (kind, flight) = sim.plan_hit(hitter, Some(HitRequest::Spike), 3, from);
    assert_eq!(kind, HitKind::Spike);
    assert!(Vec2::new(flight.landing_point().x, flight.landing_point().z).distance(spot) < 1e-3);

    let teammate_spot = Vec2::new(-2.0, 3.5);
    sim.players[hitter].position.y = 0.0;
    sim.players[hitter].aim = Some(teammate_spot);
    let (kind, flight) = sim.plan_hit(hitter, Some(HitRequest::Pass), 1, from);
    assert_eq!(kind, HitKind::Pass);
    assert!(Vec2::new(flight.landing_point().x, flight.landing_point().z).distance(teammate_spot) < 1e-3);
}

#[test]
fn passes_stay_on_your_side() {
    let target = own_side_target(0, Some(Vec2::new(8.0, 20.0)), SET_DEPTH);
    assert!(target.x < 0.0 && court::is_inside(target), "{target}");
}

#[test]
fn aiming_out_lands_out() {
    let target = over_net_target(0, Some(Vec2::new(HALF_LENGTH + 2.0, 0.0)), OVER_DEPTH);
    assert!(!court::is_inside(target), "{target}");
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
    sim.touches = Touches { team: 0, count: 3, last: Some(0) };
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
                Event::Landed { at, inside, .. } if inside && Some(court::half_owner(at.x)) == last_team => {
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
