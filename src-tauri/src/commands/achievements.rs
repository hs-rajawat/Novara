use std::sync::Arc;

use tauri::State;

use crate::db::achievements::NewAchievement;
use crate::error::AppResult;
use crate::events::AppEvent;
use crate::models::Achievement;
use crate::state::AppState;

#[tauri::command]
pub async fn list_achievements(
    game_id: String,
    state: State<'_, Arc<AppState>>,
) -> AppResult<Vec<Achievement>> {
    state.db.list_achievements(&game_id).await
}

#[derive(serde::Deserialize)]
pub struct NewAchievementInput {
    pub game_id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub points: Option<i64>,
    pub is_secret: Option<bool>,
}

#[tauri::command]
pub async fn create_achievement(
    input: NewAchievementInput,
    state: State<'_, Arc<AppState>>,
) -> AppResult<Achievement> {
    state
        .db
        .create_achievement(NewAchievement {
            game_id: &input.game_id,
            name: &input.name,
            description: input.description.as_deref(),
            category: input.category.as_deref(),
            points: input.points.unwrap_or(10),
            is_secret: input.is_secret.unwrap_or(false),
        })
        .await
}

#[tauri::command]
pub async fn toggle_achievement(
    id: String,
    state: State<'_, Arc<AppState>>,
) -> AppResult<bool> {
    let (unlocked, points) = state.db.toggle_achievement(&id).await?;
    if unlocked {
        // Look up the game id for the event payload.
        if let Ok(game_id) = sqlx::query_scalar::<_, String>(
            "SELECT game_id FROM achievements WHERE id = ?1",
        )
        .bind(&id)
        .fetch_one(&state.db.pool)
        .await
        {
            state.bus.emit(AppEvent::AchievementUnlocked {
                game_id,
                achievement_id: id,
                points,
            });
        }
    }
    Ok(unlocked)
}

#[tauri::command]
pub async fn delete_achievement(
    id: String,
    state: State<'_, Arc<AppState>>,
) -> AppResult<()> {
    state.db.delete_achievement(&id).await
}
