//! medflow-keygen — DEV/TESTING ONLY key tool. Never shipped.
//!
//! Real keys come from the MedFlow Key Maker, whose signing seed never leaves
//! the owner's phone. This tool signs with the DEV seed kept in the gitignored
//! `crates/medflow-license/dev-keys/dev.mfseed`, which only builds that still
//! trust the dev kid will accept.
//!
//! ```text
//! cargo run --features cli --bin medflow-keygen -- gen-dev-key [--kid dev-2026-09] [--out PATH] [--force]
//! cargo run --features cli --bin medflow-keygen -- machine-code [--identity STR]
//! cargo run --features cli --bin medflow-keygen -- issue --machine MF-XXXX-XXXX-XXXX-XXXX \
//!     (--months N | --years N | --days N | --lifetime) --licensee "Dr ..." [--telegram] \
//!     [--issued-at UNIX_MS] [--license-id UUID] [--seed-file PATH] [--out FILE.mflic]
//! cargo run --features cli --bin medflow-keygen -- verify (--key KEY | --key-file FILE) \
//!     [--machine MF-...] [--now UNIX_MS] [--seed-file PATH]
//! cargo run --features cli --bin medflow-keygen -- public-key [--seed-file PATH]
//! ```

use medflow_license::{
    generate_seed, issue_key, public_key_from_seed, trusted_key_rust_row, verify, KeyRequest,
    KeyStatus, MachineCode, Term, TrustStore, TrustedKey,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_DEV_KID: &str = "dev-2026-09";
const APP_MAJOR: u32 = 2;

struct Args {
    command: String,
    values: HashMap<String, String>,
    flags: Vec<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut raw = std::env::args().skip(1);
    let command = raw.next().ok_or_else(usage)?;
    let mut values = HashMap::new();
    let mut flags = Vec::new();
    let rest: Vec<String> = raw.collect();
    let mut i = 0;
    while i < rest.len() {
        let name = rest[i]
            .strip_prefix("--")
            .ok_or_else(|| format!("unexpected argument '{}'\n\n{}", rest[i], usage()))?
            .to_string();
        match name.as_str() {
            "lifetime" | "telegram" | "force" => flags.push(name),
            _ => {
                let value = rest
                    .get(i + 1)
                    .ok_or_else(|| format!("--{name} needs a value"))?;
                values.insert(name, value.clone());
                i += 1;
            }
        }
        i += 1;
    }
    Ok(Args {
        command,
        values,
        flags,
    })
}

fn usage() -> String {
    "usage: medflow-keygen <gen-dev-key|machine-code|issue|verify|public-key> [options]\n\
     (see the header of src/bin/medflow-keygen.rs)"
        .to_string()
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn default_seed_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("dev-keys")
        .join("dev.mfseed")
}

struct DevSeed {
    kid: String,
    seed: [u8; 32],
}

fn read_seed(args: &Args) -> Result<DevSeed, String> {
    let path = args
        .values
        .get("seed-file")
        .map(PathBuf::from)
        .unwrap_or_else(default_seed_path);
    let text = std::fs::read_to_string(&path).map_err(|e| {
        format!(
            "cannot read the dev seed at {} ({e}); run `gen-dev-key` first",
            path.display()
        )
    })?;
    let mut kid = None;
    let mut seed = None;
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("kid=") {
            kid = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("seed=") {
            let bytes = hex::decode(v.trim()).map_err(|_| "seed is not hex".to_string())?;
            let array: [u8; 32] = bytes
                .try_into()
                .map_err(|_| "seed must be 32 bytes".to_string())?;
            seed = Some(array);
        }
    }
    Ok(DevSeed {
        kid: kid.ok_or("seed file has no kid= line")?,
        seed: seed.ok_or("seed file has no seed= line")?,
    })
}

fn this_machine() -> Result<MachineCode, String> {
    medflow_license::host::host_machine_code()
}

fn machine_arg(args: &Args) -> Result<MachineCode, String> {
    match args.values.get("machine") {
        Some(code) => MachineCode::parse(code).map_err(|e| format!("--machine: {e}")),
        None => this_machine(),
    }
}

fn number(args: &Args, name: &str) -> Result<Option<u32>, String> {
    args.values
        .get(name)
        .map(|v| v.parse::<u32>().map_err(|_| format!("--{name} must be a whole number")))
        .transpose()
}

fn gen_dev_key(args: &Args) -> Result<(), String> {
    let path = args
        .values
        .get("out")
        .map(PathBuf::from)
        .unwrap_or_else(default_seed_path);
    if path.exists() && !args.flags.iter().any(|f| f == "force") {
        return Err(format!(
            "{} already exists; pass --force to replace it (every key it signed stops verifying once the desktop trusts the new key instead)",
            path.display()
        ));
    }
    let kid = args
        .values
        .get("kid")
        .cloned()
        .unwrap_or_else(|| DEFAULT_DEV_KID.to_string());
    let seed = generate_seed().map_err(|e| e.to_string())?;
    let public = public_key_from_seed(&seed);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let body = format!(
        "# MedFlow DEV license signing seed. PRIVATE KEY: never commit, never ship.\n\
         # Only builds whose TRUSTED_KEYS still contain kid '{kid}' accept keys it signs.\n\
         kid={kid}\nseed={}\npublic={}\n",
        hex::encode(seed),
        hex::encode(public)
    );
    std::fs::write(&path, body).map_err(|e| e.to_string())?;
    println!("wrote DEV seed to {}", path.display());
    println!("kid: {kid}");
    println!("public key (hex): {}", hex::encode(public));
    println!("desktop trusted-key row:\n    {}", trusted_key_rust_row(&kid, &public));
    Ok(())
}

fn issue(args: &Args) -> Result<(), String> {
    let dev = read_seed(args)?;
    let machine = args
        .values
        .get("machine")
        .ok_or("--machine MF-XXXX-XXXX-XXXX-XXXX is required")?;
    let machine = MachineCode::parse(machine).map_err(|e| format!("--machine: {e}"))?;
    let lifetime = args.flags.iter().any(|f| f == "lifetime");
    let term = match (
        number(args, "months")?,
        number(args, "years")?,
        number(args, "days")?,
        lifetime,
    ) {
        (Some(n), None, None, false) => Term::Months(n),
        (None, Some(n), None, false) => Term::Years(n),
        (None, None, Some(n), false) => Term::Days(n),
        (None, None, None, true) => Term::Lifetime,
        _ => return Err("pass exactly one of --months N, --years N, --days N, --lifetime".into()),
    };
    let licensee = args
        .values
        .get("licensee")
        .ok_or("--licensee \"Dr ...\" is required")?
        .clone();
    let issued_at = match args.values.get("issued-at") {
        Some(v) => v
            .parse::<i64>()
            .map_err(|_| "--issued-at must be unix milliseconds".to_string())?,
        None => now_ms(),
    };
    let mut features = Vec::new();
    if args.flags.iter().any(|f| f == "telegram") {
        features.push("telegram".to_string());
    }
    let request = KeyRequest {
        licensee,
        machine_code: machine,
        major: APP_MAJOR,
        features,
        issued_at,
        term,
        license_id: args.values.get("license-id").cloned(),
    };
    let (key, license) = issue_key(&dev.seed, &dev.kid, &request).map_err(|e| e.to_string())?;
    if let Some(out) = args.values.get("out") {
        std::fs::write(out, format!("{key}\n")).map_err(|e| e.to_string())?;
        eprintln!("wrote {out}");
    }
    eprintln!(
        "DEV key | kid {} | {} | machine {} | term {:?} | expires {:?} | features {:?}",
        license.kid, license.licensee, license.machine_code, term, license.expires_at, license.features
    );
    println!("{key}");
    Ok(())
}

fn verify_cmd(args: &Args) -> Result<(), String> {
    let dev = read_seed(args)?;
    let key = match (args.values.get("key"), args.values.get("key-file")) {
        (Some(k), None) => k.clone(),
        (None, Some(path)) => std::fs::read_to_string(path).map_err(|e| e.to_string())?,
        _ => return Err("pass --key KEY or --key-file FILE".into()),
    };
    let machine = machine_arg(args)?;
    let now = match args.values.get("now") {
        Some(v) => v.parse::<i64>().map_err(|_| "--now must be unix ms".to_string())?,
        None => now_ms(),
    };
    let keys = [TrustedKey {
        kid: &dev.kid,
        public_key: public_key_from_seed(&dev.seed),
    }];
    let trust = TrustStore {
        keys: &keys,
        revoked_kids: &[],
        revoked_license_ids: &[],
    };
    let status = verify(&trust, &key, &machine, APP_MAJOR, now);
    println!("machine: {machine}");
    println!("status: {}", status.code());
    match &status {
        KeyStatus::UnknownKid(kid) => println!("kid: {kid}"),
        KeyStatus::Malformed(why) => println!("malformed: {}", why.code()),
        _ => {}
    }
    if let Some(license) = status.license() {
        println!(
            "licensee: {} | licensed machine: {} | term: {:?} | expires: {:?} | features: {:?}",
            license.licensee,
            license.machine_code,
            license.term(),
            license.expires_at,
            license.features
        );
    }
    Ok(())
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    match args.command.as_str() {
        "gen-dev-key" => gen_dev_key(&args),
        "machine-code" => {
            let code = match args.values.get("identity") {
                Some(identity) => MachineCode::derive(identity),
                None => this_machine()?,
            };
            println!("{code}");
            Ok(())
        }
        "issue" => issue(&args),
        "verify" => verify_cmd(&args),
        "public-key" => {
            let dev = read_seed(&args)?;
            let public = public_key_from_seed(&dev.seed);
            println!("kid: {}", dev.kid);
            println!("public key (hex): {}", hex::encode(public));
            println!("{}", trusted_key_rust_row(&dev.kid, &public));
            Ok(())
        }
        _ => Err(usage()),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}
