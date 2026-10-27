use game::profile::{BattleStats, Profile};
use game::walk::WalkSim;

pub(super) fn record_round(profile: &mut Profile, walk: &mut WalkSim, stats: BattleStats) {
    walk.record_battle(profile, stats);
    if let Err(e) = profile.save() {
        eprintln!("profile autosave failed: {e:#}");
    } else {
        println!(
            "career: {} rounds, {} wins, {} kills / {} deaths",
            profile.career.rounds, profile.career.wins, profile.career.kills, profile.career.deaths
        );
    }
}
