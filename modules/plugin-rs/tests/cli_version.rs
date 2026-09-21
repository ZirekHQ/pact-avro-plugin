use std::process::Command;

#[test]
fn version_flag_prints_the_crate_version_and_exits() {
    let output = Command::new(env!("CARGO_BIN_EXE_pact-avro-plugin"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        env!("CARGO_PKG_VERSION")
    );
}
