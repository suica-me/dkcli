mod parser;

use std::{borrow::Cow, path::PathBuf, process::exit, sync::Arc, time::Duration};

use anyhow::{bail, Context, Result};
use indicatif::{HumanBytes, MultiProgress, ProgressBar, ProgressStyle};
use inquire::{
    required, validator::Validation, Confirm, Password, PasswordDisplayMode, Select, Text,
};
use log::{info, LevelFilter};
use parser::list_zoneinfo;
use reqwest::ClientBuilder;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use simplelog::{ColorChoice, Config, TermLogger, TerminalMode};
use tokio::{runtime::Runtime, time::sleep};
use zbus::{proxy, Connection, Result as zResult};

const LOCALE_LIST: &str = include_str!("../lang_select.json");

struct InstallConfig {
    offline_install: bool,
    variant: Variant,
    fullname: String,
    user: String,
    password: String,
    hostname: String,
    timezone: String,
    rtc_as_localtime: bool,
    target_part: DkPartition,
    efi_disk: Option<DkPartition>,
    locale: String,
}

#[derive(Debug, Deserialize)]
struct Dbus {
    result: DbusResult,
    data: Value,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
enum DbusResult {
    Ok,
    Error,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "status")]
enum AutoPartitionProgress {
    Pending,
    Working,
    Finish { res: Result<Value, Value> },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status")]
enum ProgressStatus {
    Pending,
    Working { step: u8, progress: u8, v: usize },
    Error(Value),
    Finish,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct Recipe {
    variants: Vec<Variant>,
    mirrors: Value,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct Variant {
    name: String,
    #[serde(rename = "dir-name")]
    dir_name: Option<String>,
    retro: bool,
    squashfs: Vec<Squashfs>,
}

#[derive(Debug, Deserialize)]
struct Device {
    model: String,
    path: String,
    size: u64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct Squashfs {
    arch: String,
    data: Option<String>,
    #[serde(rename = "downloadSize")]
    download_size: u64,
    #[serde(rename = "instSize")]
    inst_size: u64,
    path: String,
    sha256sum: String,
    inodes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DkPartition {
    path: Option<PathBuf>,
    parent_path: Option<PathBuf>,
    fs_type: Option<String>,
    size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Locale {
    lang_english: String,
    locale: String,
    lang: String,
    text: String,
    data: String,
}

#[proxy(
    interface = "io.aosc.Deploykit1",
    default_service = "io.aosc.Deploykit",
    default_path = "/io/aosc/Deploykit"
)]
trait Deploykit {
    async fn set_config(&self, field: &str, value: &str) -> zResult<String>;
    async fn get_config(&self, field: &str) -> zResult<String>;
    async fn get_progress(&self) -> zResult<String>;
    async fn reset_config(&self) -> zResult<String>;
    async fn get_list_devices(&self) -> zResult<String>;
    async fn auto_partition(&self, dev: &str) -> zResult<String>;
    async fn start_install(&self) -> zResult<String>;
    async fn get_auto_partition_progress(&self) -> zResult<String>;
    async fn get_list_partitions(&self, dev: &str) -> zResult<String>;
    async fn get_recommend_swap_size(&self) -> zResult<String>;
    async fn get_memory(&self) -> zResult<String>;
    async fn find_esp_partition(&self, dev: &str) -> zResult<String>;
    async fn cancel_install(&self) -> zResult<String>;
    async fn disk_is_right_combo(&self, dev: &str) -> zResult<String>;
    async fn ping(&self) -> zResult<String>;
    async fn get_all_esp_partitions(&self) -> zResult<String>;
    async fn reset_progress_status(&self) -> zResult<String>;
    async fn sync_disk(&self) -> zResult<String>;
    async fn sync_and_reboot(&self) -> zResult<String>;
    async fn is_lvm_device(&self, dev: &str) -> zResult<String>;
    async fn is_efi(&self) -> zResult<String>;
}

impl Dbus {
    async fn run(proxy: &DeploykitProxy<'_>, method: DbusMethod<'_>) -> Result<Self> {
        let s = match method {
            DbusMethod::SetConfig(field, value) => proxy.set_config(field, value).await?,
            DbusMethod::AutoPartition(p) => proxy.auto_partition(p).await?,
            DbusMethod::GetProgress => proxy.get_progress().await?,
            DbusMethod::StartInstall => proxy.start_install().await?,
            DbusMethod::GetAutoPartitionProgress => proxy.get_auto_partition_progress().await?,
            DbusMethod::FindEspPartition(dev) => proxy.find_esp_partition(dev).await?,
            DbusMethod::ListPartitions(dev) => proxy.get_list_partitions(dev).await?,
            DbusMethod::ListDevice => proxy.get_list_devices().await?,
            DbusMethod::GetRecommendSwapSize => proxy.get_recommend_swap_size().await?,
            DbusMethod::GetMemory => proxy.get_memory().await?,
            DbusMethod::CancelInstall => proxy.cancel_install().await?,
            DbusMethod::ResetConfig => proxy.reset_config().await?,
            DbusMethod::DiskIsRightCombo(dev) => proxy.disk_is_right_combo(dev).await?,
            DbusMethod::Ping => proxy.ping().await?,
            DbusMethod::GetAllEspPartitions => proxy.get_all_esp_partitions().await?,
            DbusMethod::ResetProgressStatus => proxy.reset_progress_status().await?,
            DbusMethod::SyncDisk => proxy.sync_disk().await?,
            DbusMethod::SyncAndReboot => proxy.sync_and_reboot().await?,
            DbusMethod::IsLvmDevice(dev) => proxy.is_lvm_device(dev).await?,
            DbusMethod::IsEFI => proxy.is_efi().await?,
        };

        let res = Self::try_from(s)?;
        Ok(res)
    }
}

#[derive(Debug)]
enum DbusMethod<'a> {
    SetConfig(&'a str, &'a str),
    AutoPartition(&'a str),
    GetProgress,
    StartInstall,
    GetAutoPartitionProgress,
    FindEspPartition(&'a str),
    ListPartitions(&'a str),
    ListDevice,
    GetRecommendSwapSize,
    GetMemory,
    CancelInstall,
    ResetConfig,
    DiskIsRightCombo(&'a str),
    Ping,
    GetAllEspPartitions,
    ResetProgressStatus,
    SyncDisk,
    SyncAndReboot,
    IsLvmDevice(&'a str),
    IsEFI,
}

impl TryFrom<String> for Dbus {
    type Error = anyhow::Error;

    fn try_from(value: String) -> std::prelude::v1::Result<Self, <Dbus as TryFrom<String>>::Error> {
        let res = serde_json::from_str::<Dbus>(&value)?;

        match res.result {
            DbusResult::Ok => Ok(res),
            DbusResult::Error => bail!("Failed to execute query: {:#?}", res.data),
        }
    }
}

fn main() -> Result<()> {
    TermLogger::init(
        LevelFilter::Debug,
        Config::default(),
        TerminalMode::Stderr,
        ColorChoice::Auto,
    )?;

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let dk_client = rt.block_on(create_dbus_client())?;
    let dk_client = Arc::new(dk_client);
    let dc = dk_client.clone();

    ctrlc::set_handler(move || {
        info!("Ctrlc is press.");
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(Dbus::run(&*dc, DbusMethod::CancelInstall))
            .unwrap();
        exit(1);
    })
    .expect("Failed to set ctrlc handler");

    let config = inquire(&rt, &dk_client)?;
    rt.block_on(set_config(&dk_client, &config))?;
    rt.block_on(Dbus::run(&dk_client, DbusMethod::StartInstall))?;
    rt.block_on(get_progress(&dk_client))?;

    Ok(())
}

async fn get_progress(dk_client: &DeploykitProxy<'_>) -> Result<()> {
    let multi_bar = MultiProgress::new();
    let style = ProgressStyle::with_template(
        "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len}",
    )?;
    let main_pb = multi_bar.add(ProgressBar::new(8).with_style(style.clone()));
    let second_pb = multi_bar.add(ProgressBar::new(100).with_style(style));

    loop {
        let progress = Dbus::run(dk_client, DbusMethod::GetProgress).await?;
        let data: ProgressStatus = serde_json::from_value(progress.data)?;

        match data {
            ProgressStatus::Working { step, progress, .. } => {
                main_pb.set_position(step as u64);
                second_pb.set_position(progress as u64);
            }
            ProgressStatus::Pending => {
                continue;
            }
            ProgressStatus::Error(e) => {
                bail!("{e}");
            }
            ProgressStatus::Finish => {
                info!("Finished");
                return Ok(());
            }
        }

        sleep(Duration::from_micros(100)).await;
    }
}

fn inquire(runtime: &Runtime, dk_client: &DeploykitProxy<'_>) -> Result<InstallConfig> {
    let is_offline_install = Confirm::new("Install AOSC OS on offline mode?")
        .with_default(true)
        .prompt()?;

    let recipe = runtime.block_on(get_recipe(is_offline_install))?;
    let variant = Select::new(
        "Install AOSC OS variant?",
        recipe
            .variants
            .iter()
            .filter(|x| !x.retro && x.name.to_lowercase() != "buildkit")
            .map(|x| x.name.to_string())
            .collect::<Vec<_>>(),
    )
    .prompt()?;

    let variant = recipe
        .variants
        .iter()
        .find(|x| x.name == variant)
        .unwrap()
        .to_owned();

    let devices = runtime.block_on(get_devices(&dk_client))?;

    println!("List of Devices:");

    for i in &devices {
        println!("{} {} ({})", i.model, i.path, HumanBytes(i.size));
    }

    let device = Select::new(
        "Select Device",
        devices.iter().map(|x| x.path.clone()).collect::<Vec<_>>(),
    )
    .prompt()?;

    let partitions = runtime.block_on(get_partitions(&dk_client, &device))?;

    let is_efi = runtime
        .block_on(Dbus::run(&dk_client, DbusMethod::IsEFI))?
        .data
        .as_bool()
        .context("Could not get is efi")?;

    info!("Device is{}EFI", if is_efi { " " } else { " not " });

    let is_lvm_device = runtime
        .block_on(Dbus::run(&dk_client, DbusMethod::IsLvmDevice(&device)))?
        .data
        .as_bool()
        .context("Could not get is lvm device")?;

    if is_lvm_device {
        bail!("Installer unsupport LVM Device.");
    }

    let partition = Select::new(
        "Select system target partition",
        partitions
            .iter()
            .filter_map(|x| x.path.as_ref().map(|x| x.to_string_lossy().to_string()))
            .collect::<Vec<_>>(),
    )
    .prompt()?;

    let partition = partitions
        .iter()
        .find(|x| {
            x.path
                .as_ref()
                .map(|x| x.to_string_lossy() == partition)
                .unwrap_or(false)
        })
        .unwrap()
        .to_owned();

    let mut efi = None;

    if is_efi {
        let efi_parts = runtime
            .block_on(Dbus::run(&dk_client, DbusMethod::GetAllEspPartitions))?
            .data;

        let efi_parts: Vec<DkPartition> = serde_json::from_value(efi_parts)?;

        if efi_parts.is_empty() {
            bail!("No ESP partition found on device");
        }

        let efi_part = Select::new(
            "Select ESP Partition",
            efi_parts
                .iter()
                .filter_map(|x| x.path.as_ref().map(|x| x.to_string_lossy().to_string()))
                .collect::<Vec<_>>(),
        )
        .prompt()?;

        let efi_part = partitions
            .iter()
            .find(|x| {
                x.path
                    .as_ref()
                    .map(|x| x.to_string_lossy() == efi_part)
                    .unwrap_or(false)
            })
            .unwrap()
            .to_owned();

        efi = Some(efi_part);
    }

    let fullname = Text::new("Your name?")
        .with_validator(required!())
        .with_validator(|input: &str| {
            if input.contains(":") {
                return Ok(Validation::Invalid("Name not allow contains ':'".into()));
            }
            return Ok(Validation::Valid);
        })
        .prompt()?;

    let mut default_username = String::new();
    for i in fullname.chars() {
        if !i.is_ascii_alphabetic() && !i.is_ascii_alphanumeric() {
            continue;
        }

        default_username.push(i.to_ascii_lowercase());
    }

    let username = Text::new("Username")
        .with_validator(required!())
        .with_validator(|input: &str| {
            for i in input.chars() {
                if !i.is_ascii_lowercase() && !i.is_ascii_alphanumeric() {
                    return Ok(Validation::Invalid(
                        format!("Username not allow contains special characters: {i}").into(),
                    ));
                }
            }
            return Ok(Validation::Valid);
        })
        .with_default(&default_username)
        .prompt()?;

    let password = Password::new("Password")
        .with_validator(required!())
        .with_display_mode(PasswordDisplayMode::Masked)
        .prompt()?;

    let timezones = list_zoneinfo()?;

    let timezone = Select::new("Select timezone", timezones).prompt()?;

    let locales: Vec<Locale> = serde_json::from_str(LOCALE_LIST)?;

    let locale = Select::new(
        "Select locale",
        locales.iter().map(|x| x.text.clone()).collect::<Vec<_>>(),
    )
    .prompt()?;

    let locale = locales.iter().find(|x| x.text == locale).unwrap();

    let hostname = Text::new("Hostname")
        .with_validator(required!())
        .with_validator(|input: &str| {
            for i in input.chars() {
                if !i.is_ascii_alphabetic() && !i.is_ascii_alphanumeric() {
                    return Ok(Validation::Invalid(
                        format!("Username not allow contains special characters: {i}").into(),
                    ));
                }
            }
            return Ok(Validation::Valid);
        })
        .prompt()?;

    let rtc_as_localtime = Confirm::new("Use RTC as localtime?")
        .with_default(false)
        .prompt()?;

    Ok(InstallConfig {
        offline_install: is_offline_install,
        variant,
        fullname,
        user: username,
        password,
        hostname,
        timezone,
        rtc_as_localtime,
        target_part: partition.into(),
        efi_disk: efi.map(|x| x.into()),
        locale: locale.data.clone(),
    })
}

async fn create_dbus_client() -> Result<DeploykitProxy<'static>> {
    let conn = Connection::system().await?;
    let client = DeploykitProxy::new(&conn).await?;

    Ok(client)
}

async fn get_recipe(offline_mode: bool) -> Result<Recipe> {
    let recipe = if !offline_mode {
        info!("Downloading Recipe file ...");
        let client = ClientBuilder::new().user_agent("deploykit").build()?;
        let resp = client
            .get("https://releases.aosc.io/manifest/recipe.json")
            .send()
            .await?
            .error_for_status()?;

        resp.json::<Recipe>().await?
    } else {
        let f = tokio::fs::read("/run/livekit/livemnt/manifest/recipe.json").await?;
        serde_json::from_slice(&f)?
    };

    Ok(recipe)
}

async fn get_devices(dk_client: &DeploykitProxy<'_>) -> Result<Vec<Device>> {
    let devices = Dbus::run(&dk_client, DbusMethod::ListDevice).await?;
    let devices: Vec<Device> = serde_json::from_value(devices.data)?;

    Ok(devices)
}

async fn get_partitions(dk_client: &DeploykitProxy<'_>, device: &str) -> Result<Vec<DkPartition>> {
    let partitions = Dbus::run(&dk_client, DbusMethod::ListPartitions(device)).await?;
    let partitions = serde_json::from_value(partitions.data)?;

    Ok(partitions)
}

async fn set_config(proxy: &DeploykitProxy<'_>, config: &InstallConfig) -> Result<()> {
    let variant = &config.variant;
    let mut sqfs = variant
        .squashfs
        .iter()
        .filter(|x| get_arch_name().map(|arch| arch == x.arch).unwrap_or(false))
        .collect::<Vec<_>>();
    sqfs.sort_unstable_by(|a, b| b.data.cmp(&a.data));
    let sqfs = sqfs.first().context("Squashfs has no entry!")?;
    let url = format!("https://releases.aosc.io/{}", sqfs.path);

    if !config.offline_install {
        let download_value = serde_json::json!({
            "Http": {
                "url": url,
                "hash": sqfs.sha256sum,
            }
        });

        Dbus::run(
            proxy,
            DbusMethod::SetConfig("download", &download_value.to_string()),
        )
        .await?;
    } else {
        let variant = config.variant.dir_name.as_ref().unwrap();

        let download_value = serde_json::json!({
            "Dir": format!("/run/livekit/sysroots/{}", variant)
        });

        Dbus::run(
            proxy,
            DbusMethod::SetConfig("download", &download_value.to_string()),
        )
        .await?;
    };

    Dbus::run(proxy, DbusMethod::SetConfig("locale", &config.locale)).await?;

    let json = serde_json::json! {{
        "username": &config.user,
        "password": &config.password,
        "full_name": &config.fullname,
    }};

    Dbus::run(proxy, DbusMethod::SetConfig("user", &json.to_string())).await?;

    Dbus::run(proxy, DbusMethod::SetConfig("timezone", &config.timezone)).await?;

    Dbus::run(proxy, DbusMethod::SetConfig("hostname", &config.hostname)).await?;
    Dbus::run(
        proxy,
        DbusMethod::SetConfig("rtc_as_localtime", &(config.rtc_as_localtime).to_string()),
    )
    .await?;

    // let swap_config = if config.swapfile.size == 0.0 {
    //     "\"Disable\"".to_string()
    // } else {
    //     serde_json::json!({
    //         "Custom": (config.swapfile.size * 1024.0 * 1024.0 * 1024.0) as u64,
    //     })
    //     .to_string()
    // };

    let swap_config = "\"Disable\"".to_string();

    Dbus::run(proxy, DbusMethod::SetConfig("swapfile", &swap_config)).await?;

    let part_config = serde_json::to_string(&config.target_part)?;

    Dbus::run(
        proxy,
        DbusMethod::SetConfig("target_partition", &part_config),
    )
    .await?;

    if let Some(efi) = &config.efi_disk {
        let part_config = serde_json::to_string(&efi)?;
        Dbus::run(proxy, DbusMethod::SetConfig("efi_partition", &part_config)).await?;
    }

    Ok(())
}

// AOSC OS specific architecture mapping for ppc64
#[cfg(target_arch = "powerpc64")]
#[inline]
pub(crate) fn get_arch_name() -> Option<&'static str> {
    let mut endian: libc::c_int = -1;
    let result;
    unsafe {
        result = libc::prctl(libc::PR_GET_ENDIAN, &mut endian as *mut libc::c_int);
    }
    if result < 0 {
        return None;
    }
    match endian {
        libc::PR_ENDIAN_LITTLE | libc::PR_ENDIAN_PPC_LITTLE => Some("ppc64el"),
        libc::PR_ENDIAN_BIG => Some("ppc64"),
        _ => None,
    }
}

/// AOSC OS specific architecture mapping table
#[cfg(not(target_arch = "powerpc64"))]
#[inline]
pub(crate) fn get_arch_name() -> Option<&'static str> {
    use std::env::consts::ARCH;
    match ARCH {
        "x86_64" => Some("amd64"),
        "x86" => Some("i486"),
        "powerpc" => Some("powerpc"),
        "aarch64" => Some("arm64"),
        "mips64" => Some("loongson3"),
        "riscv64" => Some("riscv64"),
        "loongarch64" => Some("loongarch64"),
        _ => None,
    }
}
