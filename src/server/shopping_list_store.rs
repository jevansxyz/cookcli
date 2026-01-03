use anyhow::Result;
use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use std::fs;
use std::str::FromStr;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShoppingListItem {
    pub path: String,
    pub name: String,
    pub scale: f64,
    #[serde(default)]
    pub kind: ShoppingListItemKind,
    pub quantity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ShoppingListItemKind {
    Recipe,
    Custom,
}

impl ShoppingListItemKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ShoppingListItemKind::Recipe => "recipe",
            ShoppingListItemKind::Custom => "custom",
        }
    }
}

impl Default for ShoppingListItemKind {
    fn default() -> Self {
        ShoppingListItemKind::Recipe
    }
}

impl FromStr for ShoppingListItemKind {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "recipe" => Ok(ShoppingListItemKind::Recipe),
            "custom" => Ok(ShoppingListItemKind::Custom),
            _ => Ok(ShoppingListItemKind::Recipe),
        }
    }
}

pub struct ShoppingListStore {
    file_path: Utf8PathBuf,
}

impl ShoppingListStore {
    pub fn new(base_path: &Utf8PathBuf) -> Self {
        let file_path = base_path.join(".shopping_list.txt");
        Self { file_path }
    }

    pub fn load(&self) -> Result<Vec<ShoppingListItem>> {
        if !self.file_path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&self.file_path)?;
        let mut items = Vec::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 {
                let kind = parts
                    .get(3)
                    .and_then(|p| ShoppingListItemKind::from_str(p).ok())
                    .unwrap_or_default();

                let quantity = parts
                    .get(4)
                    .map(|s| s.to_string())
                    .filter(|s| !s.trim().is_empty());

                items.push(ShoppingListItem {
                    path: parts[0].to_string(),
                    name: parts[1].to_string(),
                    scale: parts[2].parse().unwrap_or(1.0),
                    kind,
                    quantity,
                });
            }
        }

        Ok(items)
    }

    pub fn save(&self, items: &[ShoppingListItem]) -> Result<()> {
        let mut content = String::from("# Shopping List\n");
        content.push_str("# Format: path<TAB>name<TAB>scale<TAB>kind<TAB>quantity\n\n");

        for item in items {
            content.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\n",
                item.path,
                item.name,
                item.scale,
                item.kind.as_str(),
                item.quantity.clone().unwrap_or_default()
            ));
        }

        fs::write(&self.file_path, content)?;
        Ok(())
    }

    pub fn add(&self, item: ShoppingListItem) -> Result<()> {
        let mut items = self.load()?;

        // Always add as a new entry to allow multiple instances of the same recipe
        items.push(item);

        self.save(&items)?;
        Ok(())
    }

    pub fn remove(&self, path: &str) -> Result<()> {
        let mut items = self.load()?;
        // Remove only the first instance to allow removing individual entries
        if let Some(pos) = items.iter().position(|i| i.path == path) {
            items.remove(pos);
        }
        self.save(&items)?;
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        self.save(&[])?;
        Ok(())
    }
}
