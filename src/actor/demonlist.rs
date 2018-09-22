use actix::{Actor, Handler, Message, SyncContext};
use crate::{
    api::record::Submission,
    error::PointercrateError,
    model::{record::RecordStatus, Demon, Player, Record, Submitter},
    video,
};
use diesel::{
    pg::PgConnection,
    r2d2::{ConnectionManager, Pool},
    result::Error,
    RunQueryDsl,
};
use ipnetwork::IpNetwork;
use log::{debug, info};

pub const LIST_SIZE: i16 = 50;
pub const EXTENDED_LIST_SIZE: i16 = 100;

pub struct DatabaseActor(pub Pool<ConnectionManager<PgConnection>>);

impl Actor for DatabaseActor {
    type Context = SyncContext<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        info!("Started pointercrate database actor! We can now interact with the database!")
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("Stopped pointercrate database actor! We can no longer interact with the database! :(")
    }
}

pub struct SubmitterByIp(pub IpNetwork);
pub struct PlayerByName(pub String);
pub struct DemonByName(pub String);
pub struct ResolveSubmissionData(pub String, pub String);
pub struct ProcessSubmission(pub Submission, pub Submitter);
pub struct RecordById(pub i32);

impl Message for SubmitterByIp {
    type Result = Result<Submitter, PointercrateError>;
}

impl Handler<SubmitterByIp> for DatabaseActor {
    type Result = Result<Submitter, PointercrateError>;

    fn handle(&mut self, msg: SubmitterByIp, _ctx: &mut Self::Context) -> Self::Result {
        debug!("Attempt to retrieve submitter with IP '{}', creating if not exists!", msg.0);

        let connection = &*self.0.get().map_err(|_| PointercrateError::DatabaseConnectionError)?;

        match Submitter::by_ip(&msg.0).first(connection) {
            Ok(submitter) => Ok(submitter),
            Err(Error::NotFound) => Submitter::insert(connection, &msg.0).map_err(PointercrateError::database),
            Err(err) => return Err(PointercrateError::database(err)),
        }
    }
}

impl Message for PlayerByName {
    type Result = Result<Player, PointercrateError>;
}

impl Handler<PlayerByName> for DatabaseActor {
    type Result = Result<Player, PointercrateError>;

    fn handle(&mut self, msg: PlayerByName, _ctx: &mut Self::Context) -> Self::Result {
        debug!("Attempt to retrieve player with name '{}', creating if not exists!", msg.0);

        let connection = &*self.0.get().map_err(|_| PointercrateError::DatabaseConnectionError)?;

        match Player::by_name(&msg.0).first(connection) {
            Ok(player) => Ok(player),
            Err(Error::NotFound) => Player::insert(connection, &msg.0).map_err(PointercrateError::database),
            Err(err) => return Err(PointercrateError::database(err)),
        }
    }
}

impl Message for DemonByName {
    type Result = Result<Demon, PointercrateError>;
}

impl Handler<DemonByName> for DatabaseActor {
    type Result = Result<Demon, PointercrateError>;

    fn handle(&mut self, msg: DemonByName, _ctx: &mut Self::Context) -> Self::Result {
        debug!("Attempting to retrieve demon with name '{}'!", msg.0);

        let connection = &*self.0.get().map_err(|_| PointercrateError::DatabaseConnectionError)?;

        match Demon::by_name(&msg.0).first(connection) {
            Ok(demon) => Ok(demon),
            Err(Error::NotFound) =>
                Err(PointercrateError::ModelNotFound {
                    model: "Demon",
                    identified_by: msg.0,
                }),
            Err(err) => return Err(PointercrateError::database(err)),
        }
    }
}

impl Message for ResolveSubmissionData {
    type Result = Result<(Player, Demon), PointercrateError>;
}

impl Handler<ResolveSubmissionData> for DatabaseActor {
    type Result = Result<(Player, Demon), PointercrateError>;

    fn handle(&mut self, msg: ResolveSubmissionData, ctx: &mut Self::Context) -> Self::Result {
        debug!("Attempt to resolve player '{}' and demon '{}' for a submission!", msg.0, msg.1);

        let (player, demon) = (msg.0, msg.1);

        let player = self.handle(PlayerByName(player), ctx)?;
        let demon = self.handle(DemonByName(demon), ctx)?;

        Ok((player, demon))
    }
}

impl Message for ProcessSubmission {
    type Result = Result<Option<Record>, PointercrateError>;
}

impl Handler<ProcessSubmission> for DatabaseActor {
    type Result = Result<Option<Record>, PointercrateError>;

    fn handle(&mut self, msg: ProcessSubmission, ctx: &mut Self::Context) -> Self::Result {
        debug!("Processing submission {:?}", msg.0);

        if msg.1.banned {
            return Err(PointercrateError::BannedFromSubmissions)?
        }

        let Submission {
            progress,
            player,
            demon,
            video,
            verify_only,
        } = msg.0;

        let video = match video {
            Some(ref video) => Some(video::validate(video)?),
            None => None,
        };

        let (player, demon) = self.handle(ResolveSubmissionData(player, demon), ctx)?;

        if player.banned {
            return Err(PointercrateError::PlayerBanned)
        }

        if demon.position > EXTENDED_LIST_SIZE {
            return Err(PointercrateError::SubmitLegacy)
        }

        if demon.position > LIST_SIZE && progress != 100 {
            return Err(PointercrateError::Non100Extended)
        }

        if progress > 100 || progress < demon.requirement {
            return Err(PointercrateError::InvalidProgress {
                requirement: demon.requirement,
            })?
        }

        debug!("Submission is valid, checking for duplicates!");

        let connection = &*self.0.get().map_err(|_| PointercrateError::DatabaseConnectionError)?;

        let record: Result<Record, _> = match video {
            Some(ref video) => Record::get_existing(player.id, &demon.name, video).first(connection),
            None => Record::by_player_and_demon(player.id, &demon.name).first(connection),
        };

        let video_ref = video.as_ref().map(AsRef::as_ref);

        let id = match record {
            Ok(record) =>
                if record.status() != RecordStatus::Rejected && record.progress() < progress {
                    if verify_only {
                        return Ok(None)
                    }

                    if record.status() == RecordStatus::Submitted {
                        debug!(
                            "The submission is duplicated, but new one has higher progress. Deleting old with id {}!",
                            record.id
                        );

                        record.delete(connection).map_err(PointercrateError::database)?;
                    }

                    debug!("Duplicate {} either already accepted, or has lower progress, accepting!", record.id);

                    Record::insert(connection, progress, video_ref, player.id, msg.1.id, &demon.name)
                        .map_err(PointercrateError::database)?
                } else {
                    return Err(PointercrateError::SubmissionExists { status: record.status() })
                },
            Err(Error::NotFound) => {
                debug!("No duplicate found, accepting!");

                if verify_only {
                    return Ok(None)
                }

                Record::insert(connection, progress, video_ref, player.id, msg.1.id, &demon.name).map_err(PointercrateError::database)?
            },
            Err(err) => return Err(PointercrateError::database(err)),
        };

        info!("Submission successful! Created new record with ID {}", id);

        Ok(Some(Record {
            id,
            progress,
            video,
            status: RecordStatus::Submitted,
            player,
            submitter: msg.1.id,
            demon: demon.into(),
        }))
    }
}

impl Message for RecordById {
    type Result = Result<Record, PointercrateError>;
}

impl Handler<RecordById> for DatabaseActor {
    type Result = Result<Record, PointercrateError>;

    fn handle(&mut self, msg: RecordById, ctx: &mut Self::Context) -> Self::Result {
        debug!("Attempt to resolve record by id {}", msg.0);

        let connection = &*self.0.get().map_err(|_| PointercrateError::DatabaseConnectionError)?;

        match Record::by_id(msg.0).first(connection) {
            Ok(record) => Ok(record),
            Err(Error::NotFound) =>
                Err(PointercrateError::ModelNotFound {
                    model: "Record",
                    identified_by: msg.0.to_string(),
                }),
            Err(err) => return Err(PointercrateError::database(err)),
        }
    }
}
