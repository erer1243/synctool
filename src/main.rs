use eyre::{bail, ensure, Result};
use gethostname::gethostname;
use lazy_static::{initialize, lazy_static};
use std::{
    env::args,
    process::{exit, Command, Stdio},
    time::Instant,
};

const HELP_MSG: &str = "\
Arguments:
    -i    Run sync command interactively
    -s    Suspend remote computer after successful sync
    -n    Don't run sync command
    -ls   Suspend this computer after successful sync
    -ss   Shut down remote computer after successful sync
    -lss  Shut down this computer after successful sync
    -p    Print unison command
";

const IGNORES: &[&str] = &[
    "Name *.class",
    "Name *.hi",
    "Name __pycache__",
    "Name target",
    "Name License.sublime_license",
    // "Path school/linux",
    // "Path school/linux.7z",
    // Reach stuff
    "Name .stack-work",
    "Name .hie",
    "Name dist-newstyle",
    "Name node_modules",
    "Name cdk.out",
    // "Regex reach/reach-lang/docs/build",
    // "Regex reach/reach-lang/examples/.*/build",
    // "Regex reach/reach-lang/hs/t/.*/build",
    "Regex thegame/android/SDL",
    "Regex thegame/android/TheGame/app/build",
];

const LAPTOP_HOST: &str = "10.13.13.3";
const DESKTOP_HOST: &str = "10.13.13.4";
const RPI_HOST: &str = "10.13.13.6";

lazy_static! {
    static ref START: Instant = Instant::now();
}

macro_rules! log {
    ($($t:tt)*) => {
        println!("[{:.2}] {}", Instant::now().duration_since(*START).as_secs_f32(), format!($($t)*))
    };
}

#[derive(Clone, Copy)]
enum PowerAction {
    Shutdown,
    Suspend,
    Nothing,
}
use PowerAction::*;

struct SyncOptions {
    local_power: PowerAction,
    remote_power: PowerAction,
    interactive: bool,
    skip_sync: bool,
    print_unison_cmd: bool,
}

fn main() {
    initialize(&START);

    // Process CLI args
    let mut sync_options = SyncOptions {
        local_power: Nothing,
        remote_power: Nothing,
        interactive: false,
        skip_sync: false,
        print_unison_cmd: false,
    };

    for arg in args().skip(1) {
        match arg.as_str() {
            "-i" => sync_options.interactive = true,
            "-ss" => sync_options.remote_power = Shutdown,
            "-s" => sync_options.remote_power = Suspend,
            "-lss" => sync_options.local_power = Shutdown,
            "-ls" => sync_options.local_power = Suspend,
            "-n" => sync_options.skip_sync = true,
            "-p" => sync_options.print_unison_cmd = true,
            "-h" => {
                print!("{}", HELP_MSG);
                exit(0);
            }
            other => {
                println!("{} is not a valid flag", other);
                exit(1);
            }
        }
    }

    // Determine hostname and which function to use to sync
    let hostname = gethostname().into_string().unwrap();
    let sync_fn = match hostname.as_str() {
        "ism" => sync_laptop_to_desktop,
        "computinator" => sync_desktop_to_laptop,
        _ => |_: &SyncOptions| bail!("Running on unrecognized machine"),
    };

    if let Err(err) = sync_fn(&sync_options) {
        log!("{err}");
        exit(1);
    }
}

fn sync_laptop_to_desktop(sync_options: &SyncOptions) -> Result<()> {
    let do_power_actions = || -> Result<()> {
        do_remote_power_action(DESKTOP_HOST, &sync_options.remote_power)?;
        do_local_power_action(&sync_options.local_power)?;
        Ok(())
    };

    let do_sync = || -> Result<bool> {
        unison(
            DESKTOP_HOST,
            sync_options.interactive,
            sync_options.print_unison_cmd,
        )
    };

    if sync_options.skip_sync {
        log!("Skipped sync");
        wake_desktop()?;
        do_power_actions()?;
        return Ok(());
    }

    log!("Starting sync");
    if do_sync()? {
        do_power_actions()?;
        return Ok(());
    }

    wake_desktop()?;

    log!("Trying sync again");
    if do_sync()? {
        do_power_actions()?;
        return Ok(());
    }

    bail!("Sync failed");
}

fn sync_desktop_to_laptop(sync_options: &SyncOptions) -> Result<()> {
    log!("Starting sync");
    if sync_options.skip_sync
        || unison(
            LAPTOP_HOST,
            sync_options.interactive,
            sync_options.print_unison_cmd,
        )?
    {
        do_remote_power_action(LAPTOP_HOST, &sync_options.remote_power)?;
        do_local_power_action(&sync_options.local_power)?;
        Ok(())
    } else {
        bail!("Sync failed")
    }
}

// Returns Ok(true) if sync was successful, Ok(false) if sync failed.
fn unison(remote: &str, interactive: bool, print: bool) -> Result<bool> {
    let remote_folder = format!("ssh://{}//home/user/prog/", remote);
    let mut command_struct = Command::new("unison");
    let mut command = command_struct.args(["-auto", "-sshargs", "-o ConnectTimeout=8"]);

    if !interactive {
        command = command.arg("-batch");
    }

    for ignore in IGNORES {
        command = command.args(["-ignore", ignore]);
    }

    command = command.args(["/home/user/prog", remote_folder.as_str()]);
    command = command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    if print {
        let mut args = String::new();
        for a in command.get_args() {
            args.push_str(&format!("{:?} ", a));
        }
        log!("command: unison {}", args);
        exit(0);
    }

    let unison_status = command.spawn()?.wait()?;
    Ok(unison_status.success())
}

fn ping(host: &str) -> Result<bool> {
    Ok(Command::new("ping")
        .args(["-c", "3", host])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?
        .wait()?
        .success())
}

fn do_local_power_action(action: &PowerAction) -> Result<()> {
    match action {
        Shutdown => {
            log!("Shutting down this computer");
            Command::new("shutdown").output()?;
        }

        Suspend => {
            log!("Suspending this computer");
            Command::new("slp").output()?;
        }

        Nothing => {}
    }

    Ok(())
}

fn do_remote_power_action(remote: &str, action: &PowerAction) -> Result<()> {
    match action {
        Shutdown => {
            log!("Shutting down remote computer");
            Command::new("ssh")
                .args([remote, "sudo", "shutdown", "now"])
                .output()?;
        }

        Suspend => {
            log!("Suspending remote computer");
            Command::new("ssh").args([remote, "slp"]).output()?;
        }

        Nothing => {}
    }

    Ok(())
}

fn wake_desktop() -> Result<()> {
    log!("Waking desktop");
    Command::new("ssh")
        .args([RPI_HOST, "~/wake-computinator.sh"])
        .output()?;

    log!("Waiting 60 seconds for desktop to turn on");
    let mut awake = false;
    let ping_start = Instant::now();
    while Instant::now().duration_since(ping_start).as_secs_f32() < 60. {
        if ping(DESKTOP_HOST)? {
            awake = true;
            break;
        }
    }

    ensure!(awake, "Could not reach desktop");

    Ok(())
}
