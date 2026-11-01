//! Issue #15 real smoke run. Runs only with an approved plan:
//! `CREWMOOR_SMOKE_APPROVED=<sha256 of plan.json> crewmoor-smoke PLAN.json`.
//! Every external call is saved as evidence; nothing is retried.
use crewmoor_smoke::{Result, flow, run};

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match start(&args) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("smoke stopped: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
fn start(args: &[String]) -> Result<()> {
    let [plan] = args else {
        return Err("usage: crewmoor-smoke /absolute/plan.json".into());
    };
    let plan_path = std::path::Path::new(plan);
    let digest = run::sha256(plan_path)?;
    let approved = std::env::var("CREWMOOR_SMOKE_APPROVED").unwrap_or_default();
    if approved != digest {
        return Err(format!("plan {digest} is not approved; set CREWMOOR_SMOKE_APPROVED").into());
    }
    let plan: serde_json::Value = serde_json::from_slice(&std::fs::read(plan_path)?)?;
    let run = run::Run::create(plan, &digest)?;
    let result = flow::execute(&run);
    let stop = flow::finish(&run);
    run.seal()?;
    result.and(stop)
}
