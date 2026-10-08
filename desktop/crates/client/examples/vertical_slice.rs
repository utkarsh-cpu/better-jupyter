//! Real Rust consumer used by scripts/desktop_smoke.py; operates on an isolated fixture notebook.
use kernelledger_client::*;
use serde_json::json;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use uuid::Uuid;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let project = PathBuf::from(args.next().expect("project path")).canonicalize()?;
    let python = PathBuf::from(args.next().expect("runtime python"));
    let identity = ClientIdentity::load(&project.join(".kl/test-client"))?;
    let mut helper = HelperClient::start(&python, &project, &identity)?;
    let source = helper.request("source.read", json!({}))?;
    let first = &source["cells"][0];
    let id = first["id"].as_str().unwrap();
    let snapshot=helper.request("source.mutate",json!({"action":"edit","cell_id":id,"source":"print('rust vertical slice')","manifest_revision":source["manifest_revision"],"cell_revisions":{id:first["revision"]}}))?;
    start_runtime(&python, &project, None)?;
    let mut runtime = RuntimeClient::connect(&project, &identity, "control")?;
    let key = Uuid::new_v4();
    let accepted=runtime.request("run",json!({"cell_id":id,"expected_sha256":snapshot["cells"][0]["revision"],"client_request_id":key}))?;
    drop(runtime); // Recover after losing the connection, using the same durable request key.
    let mut runtime = RuntimeClient::connect(&project, &identity, "control")?;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let operation = runtime.request("operation.get", json!({"client_request_id":key}))?;
        if operation["state"] == "completed" {
            let fold = helper.request(
                "fold.get",
                json!({"exec_id":operation["result"]["exec_id"]}),
            )?;
            anyhow::ensure!(
                fold["outputs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|e| e["text"] == "rust vertical slice\n"),
                "Unexpected output"
            );
            println!(
                "{}",
                json!({"accepted":accepted,"operation":operation,"fold":fold})
            );
            break;
        }
        anyhow::ensure!(Instant::now() < deadline, "Operation did not complete");
        std::thread::sleep(Duration::from_millis(25));
    }
    Ok(())
}
