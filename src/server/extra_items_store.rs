use anyhow::Result;
use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtraItem {
    pub name: String,
    pub quantity: String,
}

pub struct ExtraItemsStore {
    file_path: Utf8PathBuf,
}

impl ExtraItemsStore {
    pub fn new(base_path: &Utf8PathBuf) -> Self {
        let file_path = base_path.join(".extra_items.txt");
        Self { file_path }
    }

    pub fn load(&self) -> Result<Vec<ExtraItem>> {
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

            let parts: Vec<&str> = line.splitn(2, '\t').collect();
            if !parts.is_empty() {
                items.push(ExtraItem {
                    name: parts[0].to_string(),
                    quantity: parts.get(1).unwrap_or(&"").to_string(),
                });
            }
        }

        Ok(items)
    }

    pub fn save(&self, items: &[ExtraItem]) -> Result<()> {
        let mut content = String::from("# Extra Shopping Items\n");
        content.push_str("# Format: name<TAB>quantity\n\n");

        for item in items {
            content.push_str(&format!("{}\t{}\n", item.name, item.quantity));
        }

        fs::write(&self.file_path, content)?;
        Ok(())
    }

    pub fn add(&self, item: ExtraItem) -> Result<()> {
        let mut items = self.load()?;
        items.push(item);
        self.save(&items)?;
        Ok(())
    }

    pub fn remove(&self, name: &str) -> Result<()> {
        let mut items = self.load()?;
        if let Some(pos) = items.iter().position(|i| i.name == name) {
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
