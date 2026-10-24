//! The vehicle and turret examples require the fresh replay's final hash to match.
pub fn check(recorded: u64, replayed: u64) -> Result<(), std::io::Error> {
    if recorded == replayed {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "replay hash mismatch: {recorded:016x} / {replayed:016x}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A real process boundary checks the Result's exit behavior used by both
    // examples, without requiring their maps, devices or a simulated defect.
    #[test]
    fn replay_result_controls_process_status() {
        for mismatch in [false, true] {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--ignored",
                    "--exact",
                    &format!(
                        "{}::replay_exit_child",
                        module_path!().split_once("::").unwrap().1
                    ),
                ])
                .env(
                    "OPENBF_TEST_REPLAY_MISMATCH",
                    if mismatch { "1" } else { "0" },
                )
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap();
            assert_eq!(status.success(), !mismatch);
        }
    }

    #[test]
    #[ignore = "subprocess entry for replay_result_controls_process_status"]
    fn replay_exit_child() -> Result<(), std::io::Error> {
        check(
            7,
            if std::env::var("OPENBF_TEST_REPLAY_MISMATCH").as_deref() == Ok("1") {
                8
            } else {
                7
            },
        )
    }
}
