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

#[test]
fn passes_stay_on_your_side() {
    let target = own_side_target(-1.0, Some(Vec2::new(8.0, 20.0)), SET_DEPTH);
    assert!(target.x < 0.0 && court::is_inside(target), "{target}");
}

#[test]
fn aiming_out_lands_out() {
    let target = over_net_target(-1.0, Some(Vec2::new(HALF_LENGTH + 2.0, 0.0)), OVER_DEPTH);
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
    sim.touches = Touches { team: 0, count: 1, last: Some(0) };
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
                Event::Landed { at, inside, .. } if inside && Some(sim.team_on(at.x)) == last_team => {
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
    sim.touches = Touches { team: 0, count: 2, last: Some(1) };
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
