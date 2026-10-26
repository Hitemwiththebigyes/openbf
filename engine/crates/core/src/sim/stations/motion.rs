fn lcg(x: u32) -> u32 {
    x.wrapping_mul(0x19660d).wrapping_add(0x3c6ef35f)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StationMotion {
    pub state: u8,
    pub timer: f32,
    pub sign: f32,
    pub wobble_freq: f32,
    pub yaw: f32,
    rng: u32,
}

impl StationMotion {
    pub fn new(seed: u32) -> Self {
        StationMotion {
            state: 0,
            timer: 0.0,
            sign: 1.0,
            wobble_freq: 0.0,
            yaw: 0.0,
            rng: seed,
        }
    }

    fn uniform(&mut self) -> f32 {
        let a = lcg(self.rng);
        self.rng = lcg(a);
        let bits = ((a >> 16) | (self.rng & 0xffff_0000)) >> 9 | 0x3f80_0000;
        f32::from_bits(bits) - 1.0
    }

    pub fn step(&mut self, anim: &super::StationAnim, serving: bool, dt: f32) {
        let busy = matches!(self.state, 2 | 3);
        if serving && !busy {
            self.state = 3;
        } else if !serving && busy {
            self.state = 1;
        }
        match self.state {
            1 => {
                self.timer -= dt;
                if self.timer >= 0.0 {
                    return;
                }
                self.state = 0;
                self.sign = -self.sign;
                if anim.rotate_speed <= 0.0001 {
                    self.timer = 0.0;
                    return;
                }
                let t = ((self.uniform() * 0.5 + 0.75) * std::f32::consts::PI) / anim.rotate_speed;
                self.timer = t;
                if anim.wobble_node.is_some() {
                    self.wobble_freq = 37.699112 / t;
                }
            }
            3 => self.timer -= dt,
            _ => {
                self.timer -= dt;
                if self.timer < 0.0 {
                    self.state = if self.state == 0 { 1 } else { 3 };
                    self.timer = 0.0;
                }
                self.yaw += self.sign * anim.rotate_speed * dt;
                if self.state == 1 {
                    self.timer = anim.wait_time;
                }
            }
        }
    }

    pub fn hash_into(&self, eat: &mut impl FnMut(&[u8])) {
        eat(&[self.state]);
        eat(&self.rng.to_le_bytes());
        for v in [self.timer, self.sign, self.wobble_freq, self.yaw] {
            eat(&v.to_bits().to_le_bytes());
        }
    }
}
