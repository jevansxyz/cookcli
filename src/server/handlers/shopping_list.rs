use crate::server::{
    shopping_list_store::{ShoppingListItem, ShoppingListItemKind, ShoppingListStore},
    AppState,
};
use crate::util::{extract_ingredients, PARSER};
use axum::{extract::State, http::StatusCode, Json};
use cooklang::ingredient_list::IngredientList;
use serde::Deserialize;
use serde_json;
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct RecipeRequest {
    recipe: String,
    scale: Option<f64>,
    #[serde(default)]
    kind: ShoppingListItemKind,
    name: Option<String>,
    quantity: Option<String>,
}

pub async fn shopping_list(
    State(state): State<Arc<AppState>>,
    axum::extract::Json(payload): axum::extract::Json<Vec<RecipeRequest>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let entries = payload;
    let mut list = IngredientList::new();
    let mut seen = BTreeMap::new();

    for entry in entries.iter() {
        if entry.kind == ShoppingListItemKind::Custom {
            continue;
        }

        let recipe_with_scale = if let Some(scale) = entry.scale {
            format!("{}:{}", entry.recipe, scale)
        } else {
            entry.recipe.clone()
        };

        extract_ingredients(
            &recipe_with_scale,
            &mut list,
            &mut seen,
            &state.base_path,
            PARSER.converter(),
            false,
        )
        .map_err(|e| {
            tracing::error!("Error processing recipe: {}", e);
            StatusCode::BAD_REQUEST
        })?;
    }

    // Load aisle configuration with lenient parsing
    let aisle_content = if let Some(path) = &state.aisle_path {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                tracing::debug!("Loaded aisle file from: {:?}", path);
                content
            }
            Err(e) => {
                tracing::warn!("Failed to read aisle file from {:?}: {}", path, e);
                String::new()
            }
        }
    } else {
        tracing::debug!("No aisle file configured");
        String::new()
    };

    // Parse aisle with lenient parsing
    let aisle_result = cooklang::aisle::parse_lenient(&aisle_content);

    if aisle_result.report().has_warnings() {
        for warning in aisle_result.report().warnings() {
            tracing::warn!("Aisle configuration warning: {}", warning);
        }
    }

    let aisle = aisle_result.output().cloned().unwrap_or_default();

    // Load pantry configuration
    let pantry_conf = if let Some(path) = &state.pantry_path {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                tracing::debug!("Loaded pantry file from: {:?}", path);
                // Parse pantry file using cooklang pantry parser
                let result = cooklang::pantry::parse_lenient(&content);

                if result.report().has_warnings() {
                    for warning in result.report().warnings() {
                        tracing::warn!("Pantry configuration warning: {}", warning);
                    }
                }

                result.output().cloned()
            }
            Err(e) => {
                tracing::warn!("Failed to read pantry file from {:?}: {}", path, e);
                None
            }
        }
    } else {
        tracing::debug!("No pantry file configured");
        None
    };

    // Use common names from aisle configuration
    list = list.use_common_names(&aisle, PARSER.converter());

    // Track pantry items that were found and subtracted (excluding zero quantities)
    let mut pantry_items = Vec::new();
    if let Some(ref pantry) = pantry_conf {
        // Check which items from the original list are in the pantry with non-zero quantity
        for (ingredient_name, _) in list.iter() {
            if let Some((_, pantry_item)) = pantry.find_ingredient(ingredient_name) {
                // Check if the pantry item has a non-zero quantity
                if let Some(qty_str) = pantry_item.quantity() {
                    // Special case for unlimited
                    if qty_str == "unlim" || qty_str == "unlimited" {
                        pantry_items.push(ingredient_name.clone());
                    } else if let Some((value, _)) = pantry_item.parsed_quantity() {
                        // Only include if quantity is greater than 0
                        if value > 0.0 {
                            pantry_items.push(ingredient_name.clone());
                        }
                    }
                } else {
                    // No quantity specified means we have it (backward compatibility)
                    pantry_items.push(ingredient_name.clone());
                }
            }
        }
    }

    // Apply pantry subtraction if pantry is available
    let final_list = if let Some(ref pantry) = pantry_conf {
        list.subtract_pantry(pantry, PARSER.converter())
    } else {
        list
    };

    let categories = final_list.categorize(&aisle);

    // Build the response
    let mut shopping_categories = Vec::new();

    for (category, items) in categories {
        let mut shopping_items = Vec::new();

        for (name, qty) in items {
            let item_json = serde_json::json!({
                "name": name,
                "quantities": qty.into_vec()
            });
            shopping_items.push(item_json);
        }

        if !shopping_items.is_empty() {
            shopping_categories.push(serde_json::json!({
                "category": category,
                "items": shopping_items
            }));
        }
    }

    // Add any custom items from the payload as their own category
    let custom_items: Vec<_> = entries
        .into_iter()
        .filter(|entry| entry.kind == ShoppingListItemKind::Custom)
        .filter_map(|entry| {
            entry
                .name
                .or_else(|| Some(entry.recipe.clone()))
                .map(|name| (name, entry.quantity))
        })
        .collect();

    if !custom_items.is_empty() {
        let mut items = Vec::new();
        for (name, quantity) in custom_items {
            let item_json = serde_json::json!({
                "name": name,
                "quantities": quantity
                    .map(|q| vec![serde_json::json!({"value": q})])
                    .unwrap_or_else(Vec::new)
            });
            items.push(item_json);
        }

        shopping_categories.push(serde_json::json!({
            "category": "Custom Items",
            "items": items
        }));
    }

    let json_value = serde_json::json!({
        "categories": shopping_categories,
        "pantry_items": pantry_items
    });
    Ok(Json(json_value))
}

pub async fn get_shopping_list_items(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ShoppingListItem>>, StatusCode> {
    let store = ShoppingListStore::new(&state.base_path);
    let items = store.load().map_err(|e| {
        tracing::error!("Failed to load shopping list: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(items))
}

#[derive(Debug, Deserialize)]
pub struct AddItemRequest {
    pub path: Option<String>,
    pub name: String,
    #[serde(default = "default_scale")]
    pub scale: f64,
    #[serde(default)]
    pub kind: ShoppingListItemKind,
    pub quantity: Option<String>,
}

fn default_scale() -> f64 {
    1.0
}

fn generate_custom_path(name: &str) -> String {
    let sanitized = name
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '-')
        .collect::<String>()
        .trim()
        .replace(' ', "-")
        .to_lowercase();

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);

    format!("custom:{}-{}", sanitized, timestamp)
}

pub async fn add_to_shopping_list(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<AddItemRequest>,
) -> Result<StatusCode, StatusCode> {
    let store = ShoppingListStore::new(&state.base_path);
    let provided_path = payload.path.as_deref().unwrap_or("").trim();

    let mut path = if payload.kind == ShoppingListItemKind::Custom {
        if provided_path.is_empty() {
            generate_custom_path(&payload.name)
        } else {
            provided_path.to_string()
        }
    } else {
        provided_path.to_string()
    };

    if path.is_empty() {
        if payload.name.trim().is_empty() {
            return Err(StatusCode::BAD_REQUEST);
        }
        path = generate_custom_path(&payload.name);
    }

    if path.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let item = ShoppingListItem {
        path,
        name: payload.name,
        scale: payload.scale,
        kind: payload.kind,
        quantity: payload.quantity,
    };

    store.add(item).map_err(|e| {
        tracing::error!("Failed to add to shopping list: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(StatusCode::OK)
}

#[derive(Debug, Deserialize)]
pub struct RemoveItemRequest {
    pub path: String,
}

pub async fn remove_from_shopping_list(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RemoveItemRequest>,
) -> Result<StatusCode, StatusCode> {
    let store = ShoppingListStore::new(&state.base_path);
    store.remove(&payload.path).map_err(|e| {
        tracing::error!("Failed to remove from shopping list: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(StatusCode::OK)
}

pub async fn clear_shopping_list(
    State(state): State<Arc<AppState>>,
) -> Result<StatusCode, StatusCode> {
    let store = ShoppingListStore::new(&state.base_path);
    store.clear().map_err(|e| {
        tracing::error!("Failed to clear shopping list: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(StatusCode::OK)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;
    use camino::Utf8PathBuf;
    use tempfile::TempDir;

    fn test_state() -> (Arc<AppState>, TempDir) {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let base_path =
            Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).expect("utf8 base path");

        (
            Arc::new(AppState {
                base_path,
                aisle_path: None,
                pantry_path: None,
            }),
            temp_dir,
        )
    }

    #[tokio::test]
    async fn add_recipe_item_with_path_succeeds() {
        let (state, _dir) = test_state();
        let payload = AddItemRequest {
            path: Some("recipes/pie.cook".to_string()),
            name: "Pie".to_string(),
            scale: 2.0,
            kind: ShoppingListItemKind::Recipe,
            quantity: None,
        };

        let status = add_to_shopping_list(State(state.clone()), Json(payload))
            .await
            .expect("add recipe");
        assert_eq!(status, StatusCode::OK);

        let items = ShoppingListStore::new(&state.base_path)
            .load()
            .expect("load store");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].path, "recipes/pie.cook");
        assert_eq!(items[0].kind, ShoppingListItemKind::Recipe);
    }

    #[tokio::test]
    async fn add_custom_item_generates_path_when_missing() {
        let (state, _dir) = test_state();
        let payload = AddItemRequest {
            path: None,
            name: "Bread".to_string(),
            scale: 1.0,
            kind: ShoppingListItemKind::Custom,
            quantity: Some("2 loaves".to_string()),
        };

        let status = add_to_shopping_list(State(state.clone()), Json(payload))
            .await
            .expect("add custom");
        assert_eq!(status, StatusCode::OK);

        let items = ShoppingListStore::new(&state.base_path)
            .load()
            .expect("load store");
        assert_eq!(items.len(), 1);
        assert!(items[0].path.starts_with("custom:"));
        assert_eq!(items[0].kind, ShoppingListItemKind::Custom);
        assert_eq!(items[0].name, "Bread");
        assert_eq!(items[0].quantity.as_deref(), Some("2 loaves"));
    }
}
