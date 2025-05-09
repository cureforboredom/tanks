use spacetimedb::rand::distributions::Alphanumeric;
use spacetimedb::rand::Rng;
use spacetimedb::{
    reducer, table, Identity, ReducerContext, ScheduleAt, Table, TimeDuration, Timestamp,
};

#[table(name = user, public)]
pub struct User {
    #[primary_key]
    identity: Identity,
    name: Option<String>,
    room: Option<u64>,
    online: bool,
}

#[table(name = room)]
pub struct Room {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[unique]
    #[index(btree)]
    key: String,
}

#[table(name = message, public)]
pub struct Message {
    #[primary_key]
    #[auto_inc]
    id: u64,
    sender: Identity,
    room: u64,
    #[index(btree)]
    sent: Timestamp,
    kind: String,
    data: Option<Vec<f64>>,
}

#[table(name = clear_messages_schedule, scheduled(clear_messages))]
struct ClearMessagesSchedule {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: ScheduleAt,
}

#[reducer]
fn clear_messages(ctx: &ReducerContext, _arg: ClearMessagesSchedule) -> Result<(), String> {
    if ctx.sender != ctx.identity() {
        return Err("Reducer `clear_messages` may not be invoked by clients".to_string());
    }
    let messages = ctx
        .db
        .message()
        .iter()
        .filter(|m| ctx.timestamp - TimeDuration::from_micros(60_000_000) > m.sent)
        .collect::<Vec<_>>();

    for message in messages {
        ctx.db.message().delete(message);
    }

    Ok(())
}

#[reducer(init)]
fn init(ctx: &ReducerContext) {
    let one_minute = TimeDuration::from_micros(60_000_000);

    ctx.db
        .clear_messages_schedule()
        .insert(ClearMessagesSchedule {
            scheduled_id: 0,
            scheduled_at: one_minute.into(),
        });
}

#[reducer]
pub fn create_room(ctx: &ReducerContext) -> Result<(), String> {
    if let Some(user) = ctx.db.user().identity().find(ctx.sender) {
        let room = loop {
            let room_key = ctx
                .rng()
                .sample_iter(Alphanumeric)
                .take(8)
                .map(char::from)
                .collect();
            let room = ctx.db.room().try_insert(Room {
                id: 0,
                key: room_key,
            });
            if room.is_ok() {
                break room.unwrap();
            }
        };
        ctx.db.user().identity().update(User {
            room: Some(room.id),
            ..user
        });
        Ok(())
    } else {
        Err("Unknown user".to_string())
    }
}

#[reducer]
pub fn join_room(ctx: &ReducerContext, room_key: String) -> Result<(), String> {
    if let Some(user) = ctx.db.user().identity().find(ctx.sender) {
        if let Some(room) = ctx.db.room().key().find(room_key.clone()) {
            ctx.db.user().identity().update(User {
                room: Some(room.id),
                ..user
            });
            Ok(())
        } else {
            Err("Room key is not valid".to_string())
        }
    } else {
        Err("Unknown user".to_string())
    }
}

#[reducer]
pub fn set_name(ctx: &ReducerContext, name: String) -> Result<(), String> {
    if name.is_empty() {
        Err("Names cannot be empty".to_string())
    } else if let Some(user) = ctx.db.user().identity().find(ctx.sender) {
        ctx.db.user().identity().update(User {
            name: Some(name.clone()),
            ..user
        });
        Ok(())
    } else {
        Err("Cannot set name for unknown user".to_string())
    }
}

#[reducer]
pub fn send_message(
    ctx: &ReducerContext,
    kind: String,
    data: Option<Vec<f64>>,
) -> Result<(), String> {
    if let Some(user) = ctx.db.user().identity().find(ctx.sender) {
        if user.room.is_some() {
            ctx.db.message().insert(Message {
                id: 0,
                sender: ctx.sender,
                room: user.room.unwrap(),
                sent: ctx.timestamp,
                kind: kind,
                data: data,
            });
            Ok(())
        } else {
            Err("User not in a room".to_string())
        }
    } else {
        Err("Unknown user".to_string())
    }
}

#[reducer(client_connected)]
pub fn client_connected(ctx: &ReducerContext) {
    if let Some(user) = ctx.db.user().identity().find(ctx.sender) {
        ctx.db.user().identity().update(User {
            online: true,
            room: Some(0),
            ..user
        });
    } else {
        ctx.db.user().insert(User {
            identity: ctx.sender,
            online: true,
            name: None,
            room: Some(0),
        });
    }
}

#[reducer(client_disconnected)]
pub fn client_disconnected(ctx: &ReducerContext) {
    if let Some(user) = ctx.db.user().identity().find(ctx.sender) {
        ctx.db.user().identity().update(User {
            online: false,
            ..user
        });
    } else {
        log::warn!(
            "Disconnect event for unknown user with identity {:?}",
            ctx.sender
        );
    }
}
