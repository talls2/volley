/* The match simulation's C interface: crates/sim_ffi/src/lib.rs, built as
 * volley_sim_ffi.dll. Keep this header in step with it; the Rust side
 * documents each function.
 *
 * The sim's world is Y up, in meters, with the net along x = 0. It runs at
 * 60 ticks a second. */

#ifndef VOLLEY_SIM_H
#define VOLLEY_SIM_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct VolleyMatch VolleyMatch;

typedef struct VolleyInput {
    float move_x, move_z;
    uint8_t has_aim;
    float aim_x, aim_z, aim_power;
    uint8_t jump, jump_released, pass, pass_held, spike, dive, kick, dash, ability, ultimate;
} VolleyInput;

enum { VOLLEY_KIT_ALL_ROUNDER = 0, VOLLEY_KIT_CROSS = 1, VOLLEY_KIT_GOLAZO = 2 };
enum { VOLLEY_MOVE_WINDUP = 0, VOLLEY_MOVE_ACTIVE = 1, VOLLEY_MOVE_RECOVERY = 2, VOLLEY_MOVE_OVER = 3 };

/* Moves, in the sim's MoveId order. */
enum {
    VOLLEY_PASS = 0, VOLLEY_SPIKE = 1, VOLLEY_DIVE = 2, VOLLEY_FOOT_SAVE = 3,
    VOLLEY_CROSSOVER = 4, VOLLEY_POSTERIZER = 5, VOLLEY_BANANA_KICK = 6, VOLLEY_CHILENA = 7,
};

typedef struct VolleyPlayerState {
    float x, y, z;    /* feet */
    float vx, vy, vz;
    uint32_t team;
    uint8_t stunned, blocking, grounded, has_action;
    uint32_t kit;
    int32_t action_move;
    uint32_t action_phase, action_start_tick;
    float action_dir_x, action_dir_z;
    int32_t active_move;
    float dash_dir_x, dash_dir_z;
    uint32_t dash_start_tick;
    uint8_t has_dash, has_carry, must_not_touch, feet;
    float carry_vx, carry_vz;
    int32_t contact_ticks;
    float contact_x, contact_y, contact_z;
    uint32_t technique;
    uint8_t no_look;
    int32_t takeoff_ticks;
    float future_x[3], future_z[3];  /* feet 0.2, 0.4 and 0.6 s ahead */
} VolleyPlayerState;

enum { VOLLEY_BALL_DEAD = 0, VOLLEY_BALL_HELD = 1, VOLLEY_BALL_IN_FLIGHT = 2, VOLLEY_BALL_CARRIED = 3 };
enum { VOLLEY_PHASE_RALLY = 0, VOLLEY_PHASE_POINT_SCORED = 1, VOLLEY_PHASE_MATCH_OVER = 2 };

typedef struct VolleyMatchState {
    uint32_t tick, rally, phase;
    uint32_t score[2], sets[2], set;
    float ball_x, ball_y, ball_z;
    uint32_t ball_state;
    int32_t ball_holder;
    float ball_radius;
    uint8_t has_landing;
    float landing_x, landing_y, landing_z;
    uint8_t has_decoy;
    float decoy_x, decoy_y, decoy_z;
    float ball_vx, ball_vy, ball_vz;
} VolleyMatchState;

enum {
    VOLLEY_EVENT_MOVE_STARTED = 0,
    VOLLEY_EVENT_DASHED = 1,
    VOLLEY_EVENT_CARRIED = 2,
    VOLLEY_EVENT_DRIBBLED = 3,
    VOLLEY_EVENT_CHAINED = 4,
    VOLLEY_EVENT_POSTERIZED = 5,
    VOLLEY_EVENT_SPLIT = 6,
    VOLLEY_EVENT_DECOY_POPPED = 7,
    VOLLEY_EVENT_TOUCHED = 8,
    VOLLEY_EVENT_BLOCKED = 9,
    VOLLEY_EVENT_HIT_NET = 10,
    VOLLEY_EVENT_LANDED = 11,
    VOLLEY_EVENT_WALL_BOUNCE = 12,
    VOLLEY_EVENT_WALL_JUMPED = 13,
    VOLLEY_EVENT_POINT = 14,
    VOLLEY_EVENT_SIDES_SWITCHED = 15,
    VOLLEY_EVENT_SET_WON = 16,
    VOLLEY_EVENT_MATCH_WON = 17,
};

/* Touched's detail, in the sim's HitKind order. */
enum {
    VOLLEY_HIT_SERVE = 0, VOLLEY_HIT_PASS = 1, VOLLEY_HIT_DIG = 2, VOLLEY_HIT_KICK = 3, VOLLEY_HIT_LOB = 4,
    VOLLEY_HIT_SPIKE = 5, VOLLEY_HIT_VOLLEY = 6, VOLLEY_HIT_BICYCLE = 7, VOLLEY_HIT_DUNK = 8, VOLLEY_HIT_CURVE = 9,
};

typedef struct VolleyEvent {
    uint32_t kind;
    int32_t player, team;
    uint32_t detail;
    float quality;
    float x, y, z;
    float vx, vy, vz;
} VolleyEvent;

VolleyMatch *volley_match_new(void);
/* Red's first player (the one a person plays) as a VOLLEY_KIT_*. */
VolleyMatch *volley_match_new_as(uint32_t kit);
void volley_match_free(VolleyMatch *game);
uint32_t volley_match_player_count(const VolleyMatch *game);
uint32_t volley_match_step(VolleyMatch *game, int32_t human, const VolleyInput *input);
uint32_t volley_match_events(const VolleyMatch *game, VolleyEvent *out, uint32_t max);
uint32_t volley_match_state(const VolleyMatch *game, VolleyMatchState *state, VolleyPlayerState *players, uint32_t max_players);
uint64_t volley_match_checksum(const VolleyMatch *game);

#ifdef __cplusplus
}
#endif

#endif
