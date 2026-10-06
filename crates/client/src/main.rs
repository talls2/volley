//! Game client: turns keyboard and gamepad input into simulation inputs, steps
//! the match at a fixed rate and draws the result. No gameplay rules live here;
//! they're all in `volley_sim`.

mod aim;
mod arena;
mod audio;
mod bench;
mod camera;
mod characters;
mod effects;
mod feel;
mod flow;
mod heroes;
mod hud;
mod input;
mod scene;
mod select;
mod trail;

use bevy::prelude::*;
use volley_sim::{Event, MatchConfig, Sim, TICK_HZ};

/// The match, plus its state one tick earlier. Rendering blends between the two
/// because the screen refreshes more often than the simulation ticks.
#[derive(Resource)]
pub struct Match {
    pub previous: Sim,
    pub current: Sim,
}

/// A simulation event, forwarded to whichever systems care (HUD, character animations).
#[derive(Message)]
pub struct SimEvent(pub Event);

fn main() {
    let sim = Sim::new(MatchConfig::default());
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "Volley".into(), ..default() }),
            ..default()
        }))
        .insert_resource(Time::<Fixed>::from_hz(TICK_HZ as f64))
        .insert_resource(Match { previous: sim.clone(), current: sim })
        .add_message::<SimEvent>()
        .add_plugins((
            input::plugin,
            camera::plugin,
            aim::plugin,
            scene::plugin,
            arena::plugin,
            trail::plugin,
            effects::plugin,
            feel::plugin,
            flow::plugin,
            select::plugin,
            characters::plugin,
            audio::plugin,
            hud::plugin,
            bench::plugin,
        ))
        .add_systems(FixedUpdate, step_match.run_if(in_state(flow::Screen::Playing)))
        .run();
}

fn step_match(mut game: ResMut<Match>, mut controls: input::Controls, mut events: MessageWriter<SimEvent>) {
    let Match { previous, current } = &mut *game;
    let inputs = controls.take_inputs(current);
    previous.clone_from(current);
    for event in current.step(&inputs) {
        events.write(SimEvent(event));
    }
}
