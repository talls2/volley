//! Sound: hits, the ball on sand, footsteps, the referee's whistle, a crowd and
//! the sea. Hits, sand and footsteps come from where they happen on the court.

use bevy::audio::{DefaultSpatialScale, SpatialScale, Volume};
use bevy::prelude::*;
use volley_sim::{DT, Event, HitKind, MoveId};

use crate::{Match, SimEvent};

/// Meters of running between footsteps.
const STRIDE: f32 = 1.3;
/// Seconds between waves, at least and at most.
const WAVE_GAP: (f32, f32) = (3.5, 7.0);

pub fn plugin(app: &mut App) {
    // Shrinks distances for stereo panning and falloff, so the far side of the
    // court is quieter but still clearly audible.
    app.insert_resource(DefaultSpatialScale(SpatialScale::new(0.15)))
        .add_systems(Startup, load_sounds)
        .add_systems(Update, (play_game_sounds, footsteps, waves).run_if(resource_exists::<Sounds>));
}

#[derive(Resource)]
struct Sounds {
    pass: Vec<Handle<AudioSource>>,
    spike: Vec<Handle<AudioSource>>,
    serve: Vec<Handle<AudioSource>>,
    block: Vec<Handle<AudioSource>>,
    sand: Vec<Handle<AudioSource>>,
    step: Vec<Handle<AudioSource>>,
    wave: Vec<Handle<AudioSource>>,
    net: Handle<AudioSource>,
    whistle: Handle<AudioSource>,
    cheer: Handle<AudioSource>,
    roar: Handle<AudioSource>,
}

fn load_sounds(mut commands: Commands, assets: Res<AssetServer>) {
    let many = |name: &str, extension: &str, range: std::ops::Range<u32>| -> Vec<Handle<AudioSource>> {
        range.map(|i| assets.load(format!("sounds/{name}_{i}.{extension}"))).collect()
    };
    commands.insert_resource(Sounds {
        pass: many("hit_pass", "ogg", 0..3),
        spike: many("hit_spike", "ogg", 0..3),
        serve: many("hit_serve", "ogg", 0..2),
        block: many("block", "ogg", 0..2),
        sand: many("sand", "ogg", 0..3),
        step: many("step", "ogg", 0..5),
        wave: many("wave", "flac", 1..5),
        net: assets.load("sounds/net.ogg"),
        whistle: assets.load("sounds/whistle.flac"),
        cheer: assets.load("sounds/crowd_cheer.flac"),
        roar: assets.load("sounds/crowd_roar.flac"),
    });
    commands.spawn((
        AudioPlayer::new(assets.load::<AudioSource>("sounds/crowd_ambience.flac")),
        PlaybackSettings::LOOP.with_volume(Volume::Linear(0.25)),
    ));
}

/// Plays a sound once, from `at` on the court if given.
fn play(commands: &mut Commands, sound: &Handle<AudioSource>, volume: f32, at: Option<Vec3>) {
    let settings = PlaybackSettings { spatial: at.is_some(), ..PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume)) };
    commands.spawn((AudioPlayer::new(sound.clone()), settings, Transform::from_translation(at.unwrap_or_default())));
}

/// Takes turns through a set of variations, so repeats don't sound identical.
fn next<'a>(sounds: &'a [Handle<AudioSource>], turn: &mut usize) -> &'a Handle<AudioSource> {
    *turn += 1;
    &sounds[*turn % sounds.len()]
}

fn play_game_sounds(
    mut commands: Commands,
    game: Res<Match>,
    sounds: Res<Sounds>,
    mut events: MessageReader<SimEvent>,
    mut turn: Local<usize>,
    mut last_hit: Local<Option<HitKind>>,
    mut rally: Local<u32>,
) {
    // The whistle that lets the server serve.
    if *rally != game.current.rally {
        *rally = game.current.rally;
        play(&mut commands, &sounds.whistle, 0.35, None);
    }
    let ball = game.current.ball_position();
    for SimEvent(event) in events.read() {
        match *event {
            Event::Touched { kind, quality, .. } => {
                *last_hit = Some(kind);
                let (set, volume) = match kind {
                    // A mishit sounds like one.
                    HitKind::Spike | HitKind::Volley | HitKind::Bicycle => (&sounds.spike, 0.4 + 0.6 * quality),
                    HitKind::Dunk => {
                        play(&mut commands, &sounds.roar, 0.7, None);
                        (&sounds.spike, 1.2)
                    }
                    HitKind::Serve => (&sounds.serve, 0.8),
                    HitKind::Pass | HitKind::Dig | HitKind::Lob => (&sounds.pass, 0.7),
                    HitKind::Kick => (&sounds.pass, 0.55),
                };
                play(&mut commands, next(set, &mut turn), volume, Some(ball));
            }
            Event::Blocked { stuffed, .. } => {
                play(&mut commands, next(&sounds.block, &mut turn), 1.0, Some(ball));
                if stuffed {
                    play(&mut commands, &sounds.roar, 0.6, None);
                }
            }
            Event::HitNet { at } => play(&mut commands, &sounds.net, 0.8, Some(at)),
            Event::Landed { at, .. } => play(&mut commands, next(&sounds.sand, &mut turn), 0.9, Some(at)),
            Event::MoveStarted { player, id: MoveId::Dive | MoveId::FootSave } => {
                play(&mut commands, next(&sounds.step, &mut turn), 1.0, Some(game.current.players[player].position));
            }
            Event::MoveStarted { .. } => {}
            // The catch: a soft slap of the palm.
            Event::Carried { .. } => play(&mut commands, next(&sounds.pass, &mut turn), 0.5, Some(ball)),
            Event::Dribbled { .. } => {}
            Event::Posterized { player } => {
                play(&mut commands, next(&sounds.block, &mut turn), 1.0, Some(game.current.players[player].position));
                play(&mut commands, &sounds.roar, 0.8, None);
            }
            Event::Dashed { player } => {
                play(&mut commands, next(&sounds.step, &mut turn), 0.8, Some(game.current.players[player].position));
            }
            Event::Point { .. } => {
                play(&mut commands, &sounds.whistle, 0.5, None);
                // Big cheers for attacks; polite applause for the rest.
                let crowd = if last_hit.is_some_and(HitKind::is_attack) { &sounds.roar } else { &sounds.cheer };
                play(&mut commands, crowd, 0.45, None);
            }
            Event::SetWon { .. } => play(&mut commands, &sounds.roar, 0.6, None),
            Event::MatchWon { .. } => play(&mut commands, &sounds.roar, 0.9, None),
            Event::SidesSwitched => {}
        }
    }
}

/// A footstep every stride while running, and a heavier one when landing a jump.
fn footsteps(
    mut commands: Commands,
    game: Res<Match>,
    sounds: Res<Sounds>,
    mut walked: Local<Vec<f32>>,
    mut turn: Local<usize>,
) {
    if walked.len() != game.current.players.len() {
        *walked = vec![0.0; game.current.players.len()];
    }
    if game.previous.rally != game.current.rally || game.previous.tick == game.current.tick {
        return;
    }
    for (i, (before, after)) in game.previous.players.iter().zip(&game.current.players).enumerate() {
        if !before.grounded() && after.grounded() {
            play(&mut commands, next(&sounds.step, &mut turn), 0.6, Some(after.position));
            continue;
        }
        if !after.grounded() {
            continue;
        }
        let moved = before.position.with_y(0.0).distance(after.position.with_y(0.0));
        // Skip teleports and lunges like dives; only running counts.
        if moved > 10.0 * DT || after.lunge(game.current.tick).is_some() {
            continue;
        }
        walked[i] += moved;
        if walked[i] > STRIDE {
            walked[i] = 0.0;
            play(&mut commands, next(&sounds.step, &mut turn), 0.25, Some(after.position));
        }
    }
}

/// Waves breaking now and then.
fn waves(mut commands: Commands, time: Res<Time>, sounds: Res<Sounds>, mut next_wave: Local<f32>, mut turn: Local<usize>) {
    let now = time.elapsed_secs();
    if now < *next_wave {
        return;
    }
    let spread = (now * 12.9898).sin().abs().fract();
    *next_wave = now + WAVE_GAP.0 + (WAVE_GAP.1 - WAVE_GAP.0) * spread;
    play(&mut commands, next(&sounds.wave, &mut turn), 0.3, None);
}
