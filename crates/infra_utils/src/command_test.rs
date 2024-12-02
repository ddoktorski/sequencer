use rstest::rstest;

use crate::command::create_shell_command;

#[rstest]
#[tokio::test]
async fn create_shell_command_example() {
    let mut ls_command = create_shell_command("ls");
    let output = ls_command.output().await.expect("Failed to execute command");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    // Package root should contain a `Cargo.toml` file.
    assert!(stdout.contains("Cargo.toml"));
}
