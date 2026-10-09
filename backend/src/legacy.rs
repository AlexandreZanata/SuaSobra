//! Transactional adoption of Django data after a logical PostgreSQL 17 -> 18 restore.
use crate::error::{AppError, Result};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

const TABLES: [(&str, &str); 11] = [
    ("identity_user", "user"),
    ("suppliers_supplier", "supplier"),
    ("suppliers_establishment", "establishment"),
    ("suppliers_pickuppoint", "pickup_point"),
    ("suppliers_event", "supplier_event"),
    ("suppliers_pickuppoint_history", "point_history"),
    ("identity_token", "identity_token"),
    ("identity_login_attempt", "login_attempt"),
    ("common_audit_fact", "audit_fact"),
    ("common_outbox_message", "outbox"),
    ("common_idempotency_record", "replay"),
];
async fn mapped(tx: &mut Transaction<'_, Postgres>, entity: &str, id: &Value) -> Result<Uuid> {
    let id = match id {
        Value::String(s) => s.clone(),
        _ => id.to_string(),
    };
    sqlx::query_scalar(
        "SELECT new_id FROM suasobra.legacy_ids WHERE entity_type=$1 AND legacy_id=$2",
    )
    .bind(entity)
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| AppError::validation("Legacy foreign key could not be mapped"))
}
/// The target must be empty. Any invalid legacy row rolls the entire import back.
pub async fn import(pool: &PgPool) -> Result<u64> {
    let mut tx = pool.begin().await?;
    super::replay::lock(&mut tx, "legacy-import").await?;
    let exists: bool = sqlx::query_scalar("SELECT to_regclass('public.identity_user') IS NOT NULL")
        .fetch_one(&mut *tx)
        .await?;
    if !exists {
        return Err(AppError::validation(
            "Restore the legacy public schema into an isolated PostgreSQL 18.6 database first",
        ));
    }
    let populated: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM suasobra.users) OR EXISTS(SELECT 1 FROM suasobra.legacy_ids)",
    )
    .fetch_one(&mut *tx)
    .await?;
    if populated {
        return Err(AppError::conflict(
            "Import requires an empty Rust target; it never overwrites application data",
        ));
    }
    let mut tables = vec![];
    let mut count = 0;
    for (table, entity) in TABLES {
        let exists: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
            .bind(format!("public.{table}"))
            .fetch_one(&mut *tx)
            .await?;
        if !exists {
            continue;
        }
        // Table names are compile-time allowlisted; no request input is interpolated.
        let rows: Vec<Value> = sqlx::query_scalar(&format!(
            "SELECT to_jsonb(t) FROM public.{table} t ORDER BY id"
        ))
        .fetch_all(&mut *tx)
        .await?;
        for row in &rows {
            let legacy = match &row["id"] {
                Value::String(s) => s.clone(),
                v => v.to_string(),
            };
            let id: Uuid = sqlx::query_scalar("SELECT uuidv7()")
                .fetch_one(&mut *tx)
                .await?;
            sqlx::query(
                "INSERT INTO suasobra.legacy_ids(entity_type,legacy_id,new_id) VALUES($1,$2,$3)",
            )
            .bind(entity)
            .bind(&legacy)
            .bind(id)
            .execute(&mut *tx)
            .await?;
            // Full legacy records, including historical event references, remain private and lossless.
            sqlx::query("INSERT INTO suasobra.legacy_records(source_table,legacy_id,record) VALUES($1,$2,$3)").bind(table).bind(&legacy).bind(row).execute(&mut *tx).await?;
        }
        count += rows.len() as u64;
        tables.push((table, entity, rows));
    }
    for (table, entity, rows) in &tables {
        for row in rows {
            let id = mapped(&mut tx, entity, &row["id"]).await?;
            match *table {
                "identity_user" => {
                    let mut next = row.clone();
                    next["id"] = json!(id);
                    next["created_at"] = row["date_joined"].clone();
                    // Defaults from the existing Django model are retained explicitly.
                    if next["roles"] == json!([]) {
                        next["roles"] = json!(["consumer"]);
                    }
                    insert_record(&mut tx, "users", &next).await?;
                }
                "suppliers_supplier" => {
                    let mut next = row.clone();
                    next["id"] = json!(id);
                    next["owner_id"] = json!(mapped(&mut tx, "user", &row["owner_id"]).await?);
                    insert_record(&mut tx, "suppliers", &next).await?;
                }
                "suppliers_establishment" => {
                    let mut next = row.clone();
                    next["id"] = json!(id);
                    next["supplier_id"] =
                        json!(mapped(&mut tx, "supplier", &row["supplier_id"]).await?);
                    insert_record(&mut tx, "establishments", &next).await?;
                }
                "suppliers_pickuppoint" => {
                    let mut data = row.clone();
                    for key in [
                        "id",
                        "establishment_id",
                        "created_at",
                        "updated_at",
                        "version",
                        "is_active",
                    ] {
                        data.as_object_mut()
                            .ok_or_else(AppError::internal)?
                            .remove(key);
                    }
                    // Decimal JSON from PostgreSQL is normalized to the Rust wire representation.
                    for key in [
                        "latitude",
                        "longitude",
                        "approx_latitude",
                        "approx_longitude",
                    ] {
                        if data[key].is_number() {
                            data[key] = json!(data[key].to_string());
                        }
                    }
                    let next = json!({"id":id,"establishment_id":mapped(&mut tx,"establishment",&row["establishment_id"]).await?,"data":data,
                        "version":row["version"],"is_active":row["is_active"],"created_at":row["created_at"],"updated_at":row["updated_at"]});
                    insert_record(&mut tx, "pickup_points", &next).await?;
                }
                "suppliers_event" => {
                    let mut next = row.clone();
                    next["id"] = json!(id);
                    next["supplier_id"] =
                        json!(mapped(&mut tx, "supplier", &row["supplier_id"]).await?);
                    next["actor_id"] = if row["actor_id"].is_null() {
                        Value::Null
                    } else {
                        json!(mapped(&mut tx, "user", &row["actor_id"]).await?)
                    };
                    insert_record(&mut tx, "supplier_events", &next).await?;
                }
                "suppliers_pickuppoint_history" => {
                    let mut next = row.clone();
                    next["id"] = json!(id);
                    next["pickup_point_id"] =
                        json!(mapped(&mut tx, "pickup_point", &row["pickup_point_id"]).await?);
                    next["actor_id"] = if row["actor_id"].is_null() {
                        Value::Null
                    } else {
                        json!(mapped(&mut tx, "user", &row["actor_id"]).await?)
                    };
                    insert_record(&mut tx, "pickup_point_history", &next).await?;
                }
                "identity_token" => {
                    let mut next = row.clone();
                    next["id"] = json!(id);
                    next["user_id"] = json!(mapped(&mut tx, "user", &row["user_id"]).await?);
                    next["used_at"] = if row["is_used"] == true {
                        row["used_at"]
                            .as_str()
                            .map(|_| row["used_at"].clone())
                            .unwrap_or_else(|| row["created_at"].clone())
                    } else {
                        Value::Null
                    };
                    insert_record(&mut tx, "identity_tokens", &next).await?;
                }
                "identity_login_attempt" => {
                    let mut next = row.clone();
                    next["id"] = json!(id);
                    insert_record(&mut tx, "login_attempts", &next).await?;
                }
                "common_outbox_message" => {
                    let mut next = row.clone();
                    next["id"] = json!(id);
                    insert_record(&mut tx, "outbox", &next).await?;
                }
                // Legacy audits/replay are retained exactly in legacy_records, with ID maps.
                // They cannot authorize replay in the new UUID/command protocol.
                "common_audit_fact" | "common_idempotency_record" => {}
                _ => return Err(AppError::internal()),
            }
        }
    }
    // No secret token, password hash, email or raw record is returned or logged.
    tx.commit().await?;
    Ok(count)
}
async fn insert_record(tx: &mut Transaction<'_, Postgres>, table: &str, row: &Value) -> Result<()> {
    if ![
        "users",
        "suppliers",
        "establishments",
        "pickup_points",
        "supplier_events",
        "pickup_point_history",
        "identity_tokens",
        "login_attempts",
        "outbox",
    ]
    .contains(&table)
    {
        return Err(AppError::internal());
    }
    sqlx::query(&format!(
        "INSERT INTO suasobra.{table} SELECT (jsonb_populate_record(NULL::suasobra.{table},$1)).*"
    ))
    .bind(row)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
