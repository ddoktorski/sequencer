use std::path::PathBuf;
use std::process::Stdio;

use infra_utils::command::create_shell_command;
use infra_utils::path::resolve_project_relative_path;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::process::Child;
use tokio::task::{self, JoinHandle};
use tracing::{error, info, instrument};

pub const NODE_EXECUTABLE_PATH: &str = "target/debug/starknet_sequencer_node";

pub struct NodeRunner {
    description: String,
    index: usize,
}

impl NodeRunner {
    pub fn new(index: usize) -> Self {
        Self { description: format! {"Node ID {}:", index}, index }
    }

    pub fn get_description(&self) -> String {
        self.description.to_string()
    }
}

pub fn spawn_run_node(node_config_path: PathBuf, node_runner: NodeRunner) -> JoinHandle<()> {
    task::spawn(async move {
        info!("Running the node from its spawned task.");
        // Obtain both handles, as the processes are terminated when their handles are dropped.
        let (mut node_handle, _annotator_handle) =
            spawn_node_child_process(node_config_path, node_runner).await;
        let _node_run_result = node_handle.
            wait(). // Runs the node until completion, should be running indefinitely.
            await; // Awaits the completion of the node.
        panic!("Node stopped unexpectedly.");
    })
}

#[instrument(skip(node_runner))]
async fn spawn_node_child_process(
    node_config_path: PathBuf,
    node_runner: NodeRunner,
) -> (Child, Child) {
    info!("Getting the node executable.");
    let node_executable = get_node_executable_path();

    // get sleep amount as integer from env var
    info!("Running the node from: {}", node_executable);
    let start_sleep_dur_env_var: u64 = std::env::var("START_SLEEP_DURATION")
        .unwrap_or("30".to_string())
        .parse()
        .expect("START_SLEEP_DURATION env var not a number");
    let sleep_dur_env_var: u64 = std::env::var("SLEEP_DURATION")
        .unwrap_or("30".to_string())
        .parse()
        .expect("SLEEP_AMOUNT env var not a number");

    let mut command = node_executable;
    // let mut command = format!("sleep {} && ", if node_runner.index == 2 { env_var } else { 0 });
    // command.push_str(&node_executable);
    command.push_str(" --config_file ");
    command.push_str(node_config_path.to_str().unwrap());
    if node_runner.index == 2 {
        command.push_str(&format!(
            " & pid=$!; sleep {start_sleep_dur_env_var}; kill -STOP $pid; sleep \
             {sleep_dur_env_var}; kill -CONT $pid; sleep 1000"
        ));
    }
    let mut node_cmd: Child = create_shell_command("sh").arg("-c").arg(command)
    // let mut node_cmd: Child = create_shell_command(&node_executable)
        // .arg("--config_file")
        // .arg(node_config_path.to_str().unwrap())
        .stderr(Stdio::inherit())
        .stdout(Stdio::piped())
        .kill_on_drop(true) // Required for stopping when the handle is dropped.
        .spawn()
        .expect("Spawning sequencer node should succeed.");

    let mut annotator_cmd: Child = create_shell_command("awk")
        .arg("-v")
        // Print the prefix in different colors.
        .arg(format!("prefix=\u{1b}[3{}m{}\u{1b}[0m", node_runner.index+1, node_runner.get_description()))
        .arg("{print prefix, $0}")
        .stdin(std::process::Stdio::piped())
        .stderr(Stdio::inherit())
        .stdout(Stdio::inherit())
        .kill_on_drop(true) // Required for stopping when the handle is dropped.
        .spawn()
        .expect("Spawning node output annotation should succeed.");

    // Get the node stdout and the annotator stdin.
    let node_stdout = node_cmd.stdout.take().expect("Node stdout should be available.");
    let mut annotator_stdin =
        annotator_cmd.stdin.take().expect("Annotator stdin should be available.");

    // Spawn a task to connect the node stdout with the annotator stdin.
    tokio::spawn(async move {
        // Copy data from node stdout to annotator stdin.
        if let Err(e) =
            tokio::io::copy(&mut BufReader::new(node_stdout), &mut annotator_stdin).await
        {
            error!("Error while copying from node stdout to annotator stdin: {}", e);
        }

        // Close the annotator stdin when done.
        if let Err(e) = annotator_stdin.shutdown().await {
            error!("Failed to shut down annotator stdin: {}", e);
        }
    });

    (node_cmd, annotator_cmd)
}

pub fn get_node_executable_path() -> String {
    resolve_project_relative_path(NODE_EXECUTABLE_PATH).map_or_else(
        |_| {
            error!(
                "Sequencer node binary is not present. Please compile it using 'cargo build --bin \
                 starknet_sequencer_node' command."
            );
            panic!("Node executable should be available");
        },
        |path| path.to_string_lossy().to_string(),
    )
}
