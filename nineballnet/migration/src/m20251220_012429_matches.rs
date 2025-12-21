use loco_rs::schema::*;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.create_table(
            Table::create()
                .table(Matches::Table)
                .if_not_exists()
                .col(ColumnDef::new(Matches::Id).integer().not_null().auto_increment().primary_key())
                .col(ColumnDef::new(Matches::MatchId).uuid().not_null())
                .col(ColumnDef::new(Matches::PlayerId).uuid().not_null())
                .col(ColumnDef::new(Matches::Status).string().not_null()) // e.g. "searching", "ready"
                .col(ColumnDef::new(Matches::GatewayUrl).string())        // Nullable (until match found)
                .col(ColumnDef::new(Matches::HandoffToken).text())        // Nullable (until match found)
                .col(ColumnDef::new(Matches::CreatedAt).date_time().not_null())
                .col(ColumnDef::new(Matches::UpdatedAt).date_time().not_null())
                .to_owned(),
        ).await
    }

    async fn down(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.drop_table(Table::drop().table(Matches::Table).to_owned()).await
    }
}

#[derive(Iden)]
pub enum Matches {
    Table,
    Id,
    MatchId,
    PlayerId,
    Status,
    GatewayUrl,
    HandoffToken,
    CreatedAt,
    UpdatedAt,
}