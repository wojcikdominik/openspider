use super::{
    policy::{self, Disabled, Outcome},
    AvailableUpdate, DisableReason, UpdateContext, UpdateState,
};
pub async fn check(ctx: &UpdateContext, manual: bool) -> UpdateState {
    match policy::check(ctx, manual).await {
        Ok(Outcome::Disabled(reason)) => UpdateState::Disabled(match reason {
            Disabled::Settings => DisableReason::Settings,
            Disabled::Environment => DisableReason::Environment,
            Disabled::MissingPublicKey => DisableReason::MissingPublicKey,
            Disabled::InvalidPublicKey => DisableReason::InvalidPublicKey,
            Disabled::UnknownInstaller => DisableReason::UnknownInstaller,
        }),
        Ok(Outcome::Throttled) => UpdateState::Idle,
        Ok(Outcome::Current) => UpdateState::UpToDate,
        Ok(Outcome::Available(m)) => UpdateState::Available(AvailableUpdate {
            version: m.version.to_string(),
            notes: m.notes,
            pub_date: m.pub_date,
        }),
        Err(e) => UpdateState::Failed(e),
    }
}
