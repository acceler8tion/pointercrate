use super::Submitter;
use crate::{
    operation::{deserialize_non_optional, Hotfix, Patch},
    permissions::PermissionsSet,
    schema::submitters,
    Result,
};
use diesel::{ExpressionMethods, PgConnection, RunQueryDsl};
use log::info;
use serde_derive::Deserialize;

make_patch! {
    struct PatchSubmitter {
        banned: bool
    }
}

impl Hotfix for PatchSubmitter {
    fn required_permissions(&self) -> PermissionsSet {
        perms!(ListModerator or ListAdministrator)
    }
}

impl Patch<PatchSubmitter> for Submitter {
    fn patch(mut self, patch: PatchSubmitter, connection: &PgConnection) -> Result<Self> {
        info!("Patching player {} with {}", self.id, patch);

        patch!(self, patch: banned);

        diesel::update(submitters::table)
            .filter(submitters::submitter_id.eq(&self.id))
            .set(submitters::banned.eq(&self.banned))
            .execute(connection)?;

        Ok(self)
    }
}