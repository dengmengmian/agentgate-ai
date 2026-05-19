use rusqlite::{params, Connection};

use crate::errors::AppError;
use crate::models::pet::{PetSettings, UpdatePetSettingsInput};

pub fn get(conn: &Connection) -> Result<PetSettings, AppError> {
    conn.query_row(
        "SELECT pet_type, visible, pos_x, pos_y FROM pet_settings WHERE id = 1",
        [],
        |row| {
            Ok(PetSettings {
                pet_type: row.get(0)?,
                visible: row.get(1)?,
                pos_x: row.get(2)?,
                pos_y: row.get(3)?,
            })
        },
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => {
            AppError::internal("Pet settings not initialized")
        }
        other => AppError::database(other),
    })
}

pub fn update(
    conn: &Connection,
    input: UpdatePetSettingsInput,
) -> Result<PetSettings, AppError> {
    let existing = get(conn)?;

    let pet_type = input.pet_type.unwrap_or(existing.pet_type);
    let visible = input.visible.unwrap_or(existing.visible);
    let pos_x = input.pos_x.unwrap_or(existing.pos_x);
    let pos_y = input.pos_y.unwrap_or(existing.pos_y);

    conn.execute(
        "UPDATE pet_settings SET pet_type=?1, visible=?2, pos_x=?3, pos_y=?4 WHERE id = 1",
        params![&pet_type, visible, pos_x, pos_y],
    )?;

    get(conn)
}
