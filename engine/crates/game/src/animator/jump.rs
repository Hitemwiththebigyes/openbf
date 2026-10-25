use super::{AnimInput, Body, Game, Rig, Special};
use crate::skin::Clip;

pub(super) const BLEND_UPDATES: f32 = 30.0;
const JUMP_POSE_SECONDS: f32 = 0.3;
const HARD_LANDING_AFTER: f32 = 2.0;
const LANDING_INTERRUPT_AFTER: f32 = 0.15;
const LANDING_INTERRUPT_SPEED: f32 = 1.0;
const EXIT_BLEND_SECONDS: f32 = 0.3;

#[derive(Debug, Clone, Default)]
pub(super) struct Jump {
    air: Option<f32>,
    landing: Option<Landing>,
    exit_elapsed: Option<f32>,
}

#[derive(Debug, Clone)]
struct Landing {
    hard: bool,
    elapsed: f32,
    length: f32,
}

impl Jump {
    pub(super) fn step(
        &mut self,
        rig: &Rig,
        clips: &[Clip],
        i: &AnimInput,
        mut stance: Result<usize, Special>,
        dt: f32,
    ) -> Result<usize, Special> {
        if rig.game != Game::Bf1 {
            *self = Self::default();
            return stance;
        }
        let dt = dt.max(0.0);
        if let Some(elapsed) = &mut self.exit_elapsed {
            *elapsed += dt;
            if *elapsed >= EXIT_BLEND_SECONDS {
                self.exit_elapsed = None;
            }
        }
        match stance {
            Err(Special::Jump | Special::Fall) => {
                self.landing = None;
                self.exit_elapsed = None;
                let elapsed = self.air.get_or_insert(0.0);
                if stance == Err(Special::Jump) && *elapsed > JUMP_POSE_SECONDS {
                    stance = Err(Special::Fall);
                }
                *elapsed += dt;
            }
            Ok(_) => {
                if let Some(air) = self.air.take() {
                    let hard = air > HARD_LANDING_AFTER;
                    let lower = rig
                        .body_slots(i.set, Body::Special(Special::Landing(hard)), false)
                        .1;
                    self.landing = lower.and_then(|slot| {
                        let length = clips.get(slot.clip)?.frames as f32 / 30.0;
                        (length > 0.0).then_some(Landing {
                            hard,
                            elapsed: 0.0,
                            length,
                        })
                    });
                }
                if let Some(landing) = &mut self.landing {
                    if landing.elapsed >= landing.length {
                        self.landing = None;
                    } else if !landing.hard
                        && landing.elapsed > LANDING_INTERRUPT_AFTER
                        && i.speed() > LANDING_INTERRUPT_SPEED
                    {
                        self.exit_elapsed = Some(landing.elapsed + dt);
                        self.landing = None;
                    } else {
                        stance = Err(Special::Landing(landing.hard));
                        landing.elapsed += dt;
                    }
                }
            }
            Err(_) => *self = Self::default(),
        }
        stance
    }

    pub(super) fn fade_factor(&self) -> f32 {
        self.exit_elapsed
            .map_or(1.0, |elapsed| (elapsed / EXIT_BLEND_SECONDS).min(1.0))
    }

    pub(super) fn fading(&self) -> bool {
        self.exit_elapsed.is_some()
    }
}
