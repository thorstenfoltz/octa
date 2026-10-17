use super::*;

#[test]
fn lane_note_matches_lane() {
    assert_eq!(
        lane_note_key(choose_lane(DbEngine::Postgres, DbEngine::MySql)),
        "db.copy_lane_fast"
    );
    assert_eq!(
        lane_note_key(choose_lane(DbEngine::Postgres, DbEngine::Snowflake)),
        "db.copy_lane_universal"
    );
}
