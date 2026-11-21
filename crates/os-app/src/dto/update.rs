//! Update IPC contracts (M3.md 3.2).
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum DisableReason {
    Settings,
    Environment,
    MissingPublicKey,
    InvalidPublicKey,
    NotImplemented,
    // Windows: Installationsart (MSI/NSIS) nicht erkannt; die UI verweist auf den manuellen Download.
    UnknownInstaller,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AvailableUpdate {
    pub version: String,
    pub notes: String,
    pub pub_date: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PendingInstall {
    pub version: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum InstallWhen {
    Now,
    OnQuit,
    AfterCrawl,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum UpdateState {
    Disabled(DisableReason),
    Idle,
    Checking,
    UpToDate,
    Available(AvailableUpdate),
    Downloading { done: u64, total: Option<u64> },
    Ready(PendingInstall),
    Failed(String),
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInstallArgs {
    pub when: InstallWhen,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSetAutoArgs {
    pub check: bool,
}

/// App-wide settings wire format; follows AppSettings.updates field for field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsDto {
    pub check: bool,
    pub last_check: Option<String>,
    #[ts(optional = nullable)]
    pub skipped_version: Option<String>,
    /// OUT-254: raw stored value (`stable`, `beta`, or an invalid value kept as is); absent = standard rule.
    #[ts(optional = nullable)]
    pub channel: Option<String>,
}

/// OUT-254 / UI-131: update channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum UpdateChannel {
    Stable,
    Beta,
}
/// Arguments of `update.set_channel` (UI-131).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSetChannelArgs {
    pub channel: UpdateChannel,
}
/// Effective update channel and running version (`update.channel`, `update.set_channel`; UI-131).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UpdateChannelState {
    /// Effective channel (stored value, otherwise the standard rule of OUT-254).
    pub channel: UpdateChannel,
    /// Running version without build metadata.
    pub version: String,
    /// The running version is a pre-release (SemVer pre-release part).
    pub prerelease: bool,
}
