use super::*;
use crate::sim::{buttons, InputLog, PadState};
use crate::unit::UnitSpec;
use glam::Vec3;

fn fixture() -> Sim {
    let mut sim = Sim::new(42);
    sim.heightfield = Some(crate::sim::tests::flat_ground());
    sim.spawn(&UnitSpec::trooper(1, Vec3::new(40.0, 0.0, 0.0)));
    sim.add_cp(
        crate::sim::tests::sphere_cp("post", Vec3::ZERO, 10.0, false),
        Some(0),
    );
    sim
}

fn replay(log: &InputLog, full: bool) -> Vec<u8> {
    let mut sim = fixture();
    let mut trace = TraceWriter::new(Vec::new(), "flat-fixture", log.seed, full).unwrap();
    let mut untraced = fixture();
    for pads in log.frames() {
        sim.step(pads);
        trace.write_tick(&sim).unwrap();
        untraced.step(pads);
        assert_eq!(
            sim.hash(),
            untraced.hash(),
            "observational at tick {}",
            sim.tick
        );
    }
    trace.finish().unwrap()
}

#[test]
fn replay_600_ticks_and_one_pad_byte_first_divergence() {
    let mut log = InputLog {
        seed: 42,
        slots: 1,
        ticks: vec![PadState::default(); 600],
    };
    for full in [false, true] {
        let a = replay(&log, full);
        assert_eq!(a, replay(&log, full));
        // Only the low button byte changes, in the 300th input frame.
        log.ticks[299].buttons = buttons::JUMP;
        let b = replay(&log, full);
        let a = String::from_utf8(a).unwrap();
        let b = String::from_utf8(b).unwrap();
        let (left, right) = a.lines().zip(b.lines()).find(|(a, b)| a != b).unwrap();
        assert!(
            left.starts_with("300 ") || left.starts_with("301 "),
            "{left}"
        );
        eprintln!("first difference (full={full}):\n{left}\n{right}");
        log.ticks[299].buttons = 0;
    }
}

#[test]
fn selected_fields_change_only_their_group() {
    let baseline = fixture();
    let hashes = |s: &Sim| groups(s, false).unwrap().map(|g| g.hash);
    let before = hashes(&baseline);
    let check = |s: Sim, group: usize| {
        let after = hashes(&s);
        for i in 0..5 {
            assert_eq!(before[i] != after[i], i == group, "group {i}");
        }
    };
    let mut s = fixture();
    s.units[0].health.cur -= 1.0;
    check(s, 0);
    let mut s = fixture();
    s.units[0].soldier.pos.x += 0.125;
    check(s, 0);
    let mut s = fixture();
    s.units[0].alive = false;
    check(s, 0);
    let mut s = fixture();
    s.units[0].gen += 1;
    let h = hashes(&s);
    assert_ne!(h[0], before[0]);
    assert_ne!(h[4], before[4]);
    let mut s = fixture();
    s.lcg = crate::lcg::GameLcg::new(43);
    check(s, 1);
    let mut s = fixture();
    s.battle.teams[1].reinforcements -= 1.0;
    check(s, 2);
    let mut s = fixture();
    s.units[0].kills += 1;
    check(s, 2);
    let mut s = fixture();
    s.units[1].deaths += 1;
    check(s, 2);
    let mut s = fixture();
    s.cps[0].owner = Some(1);
    check(s, 3);
    let mut s = fixture();
    s.cps[0].capture += 0.125;
    check(s, 3);
    let mut s = fixture();
    s.cps[0].neutral += 0.125;
    check(s, 3);
    let mut s = fixture();
    s.units[0].respawn_at = Some(300);
    check(s, 4);
    let mut s = fixture();
    s.time_limit = Some(100);
    check(s, 4);
    let mut s = fixture();
    s.domination = Some((0, 2.0));
    check(s, 4);
    let mut s = fixture();
    s.waves[0].timer += 0.125;
    check(s, 4);
    let mut s = fixture();
    s.tick += 1;
    check(s, 4);
    let mut s = fixture();
    s.units[0].soldier.pos.x += POSITION_QUANTUM * 0.25;
    assert_eq!(hashes(&s), before, "sub-quantum control");
}

#[test]
fn quantization_and_wire_hash_have_independent_known_answers() {
    assert_eq!(quantize(-0.03125, POSITION_QUANTUM).unwrap(), -1);
    assert_eq!(quantize(0.03125, POSITION_QUANTUM).unwrap(), 1);
    assert_eq!(quantize(-0.125, HEALTH_QUANTUM).unwrap(), -1);
    assert_eq!(quantize(-0.0, HEALTH_QUANTUM).unwrap(), 0);
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::MAX] {
        assert!(quantize(bad, POSITION_QUANTUM).is_err());
    }
    // Standard FNV-1a reference vector: bytes "hello".
    let mut g = Group::new(false);
    for byte in b"hello" {
        g.u8(*byte);
    }
    assert_eq!(g.hash, 0xa430d84680aabd0b);
    // u32 0x6c6c6568 followed by u8 'o' must be the same bytes.
    let mut g = Group::new(false);
    g.u32(0x6c6c6568);
    g.u8(b'o');
    assert_eq!(g.hash, 0xa430d84680aabd0b);
}

#[test]
fn limits_header_injection_and_io_errors_are_not_silent() {
    assert!(TraceWriter::new(Vec::new(), "bad\n# seed 0", 42, false).is_err());
    let mut w = TraceWriter::new(Vec::new(), "fixture", 42, true).unwrap();
    w.bytes = FULL_BYTE_LIMIT;
    assert!(w.write_tick(&fixture()).is_err());
    assert!(TraceWriter::new(
        std::io::Cursor::new(&mut [0u8; 1][..]),
        "fixture",
        42,
        false
    )
    .is_err());
}

#[test]
fn original_hash_pins_without_tracing() {
    // Card DTEAM (2026-10-28) moved both: one human against none, so the
    // hash eats the per-team bot rows (`Sim::team_rows`).
    let mut sim = Sim::new(42);
    for _ in 0..100 {
        sim.step(&[PadState::default()]);
    }
    assert_eq!(sim.hash(), 0x26e3_85b8_a5a4_9460);
    for _ in 100..600 {
        sim.step(&[PadState::default()]);
    }
    assert_eq!(sim.hash(), 0xc149_3672_b357_563b);
}
