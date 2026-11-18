use std::collections::BTreeMap;

use eframe::egui;

use crate::{compiler::TextureAsset, config::Profile};

use super::icons;
use super::texture_rules::Selection;

#[derive(Default)]
struct Node {
    path: String,
    asset: Option<usize>,
    children: BTreeMap<String, Self>,
    open: Option<bool>,
}

impl Node {
    fn set_open(&mut self, open: bool) {
        self.open = Some(open);
        for child in self.children.values_mut() {
            child.set_open(open);
        }
    }

    fn show(
        &mut self,
        ui: &mut egui::Ui,
        profile: &Profile,
        selection: Option<&Selection>,
        clicked: &mut Option<Selection>,
    ) {
        let selected = match selection {
            Some(Selection::Directory(path) | Selection::Asset(path)) => path == &self.path,
            _ => false,
        };
        let rules = if self.asset.is_some() {
            &profile.assets
        } else {
            &profile.directories
        };
        let name = self.path.rsplit('/').next().unwrap_or(&self.path);
        let mut label = egui::text::LayoutJob::default();
        let format = egui::TextFormat::simple(
            egui::TextStyle::Body.resolve(ui.style()),
            if selected {
                if self.asset.is_some() {
                    super::theme::INK
                } else {
                    super::theme::MINT
                }
            } else {
                ui.visuals().text_color()
            },
        );
        label.append(
            if self.asset.is_some() {
                icons::FILE
            } else {
                icons::PROJECT
            },
            0.0,
            format.clone(),
        );
        label.append("  ", 0.0, format.clone());
        label.append(name, 0.0, format.clone());
        if rules.contains_key(&self.path) {
            label.append(" *", 0.0, format);
        }
        // Заголовок папки принудительно использует Extend; готовый galley сохраняет усечение.
        label.wrap.max_width = (-2.0f32)
            .mul_add(
                ui.spacing().button_padding.x,
                ui.available_width() - ui.spacing().indent,
            )
            .max(20.0);
        label.wrap.max_rows = 1;
        label.wrap.break_anywhere = true;
        let label = ui.painter().layout_job(label);
        ui.push_id(&self.path, |ui| {
            if self.asset.is_some() {
                ui.indent("asset", |ui| {
                    if ui
                        .add(egui::Button::selectable(selected, label).truncate())
                        .on_hover_text(&self.path)
                        .clicked()
                    {
                        *clicked = Some(Selection::Asset(self.path.clone()));
                    }
                });
            } else {
                let response = egui::CollapsingHeader::new(label)
                    .id_salt("folder")
                    .show_background(selected)
                    .open(self.open.take())
                    .show(ui, |ui| {
                        for child in self.children.values_mut() {
                            child.show(ui, profile, selection, clicked);
                        }
                    });
                if response.header_response.on_hover_text(&self.path).clicked() {
                    *clicked = Some(Selection::Directory(self.path.clone()));
                }
            }
        });
    }
}

#[derive(Default)]
pub(super) struct AssetTree {
    roots: BTreeMap<String, Node>,
}

impl AssetTree {
    pub fn update(&mut self, assets: &[TextureAsset], matches: &[(usize, u32)]) {
        self.roots.clear();
        for &(index, _) in matches {
            let path = &assets[index].path;
            let mut children = &mut self.roots;
            let mut end = 0;
            for name in path.split('/') {
                end += name.len();
                let node = children.entry(name.to_owned()).or_insert_with(|| Node {
                    path: path[..end].to_owned(),
                    ..Node::default()
                });
                if end == path.len() {
                    node.asset = Some(index);
                }
                children = &mut node.children;
                end += 1;
            }
        }
    }

    pub fn expand_all(&mut self) {
        self.set_open(true);
    }

    pub fn collapse_all(&mut self) {
        self.set_open(false);
    }

    fn set_open(&mut self, open: bool) {
        for node in self.roots.values_mut() {
            node.set_open(open);
        }
    }

    fn show(
        &mut self,
        ui: &mut egui::Ui,
        profile: &Profile,
        selection: Option<&Selection>,
    ) -> Option<Selection> {
        let mut clicked = None;
        for node in self.roots.values_mut() {
            node.show(ui, profile, selection, &mut clicked);
        }
        clicked
    }
}

impl super::Studio {
    pub(super) fn draw_asset_tree(&mut self, ui: &mut egui::Ui, busy: bool) {
        let Some(plan) = &self.plan else {
            return;
        };
        let clicked = egui::ScrollArea::vertical()
            .id_salt("asset-tree")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 3.0;
                ui.spacing_mut().button_padding.y = 3.0;
                ui.spacing_mut().interact_size.y = 24.0;
                ui.add_enabled_ui(!busy, |ui| {
                    self.asset_tree
                        .show(ui, &self.profile, self.rule_selection.as_ref())
                })
                .inner
            })
            .inner;
        if let Some(selection) = clicked {
            self.selected = match &selection {
                Selection::Asset(path) => {
                    self.asset_search.matches.iter().find_map(|&(index, _)| {
                        (plan.textures[index].path == *path).then_some(index)
                    })
                }
                Selection::Directory(_) | Selection::Category(_) => None,
            };
            self.rule_selection = Some(selection);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Category;

    #[test]
    fn folders_preserve_path_boundaries_and_filtered_ancestors() {
        let assets: Vec<_> = ["gfx/a/z.dds", "gfx/ab/x.dds", "gfx/a/b/y.dds", "gfx/a.dds"]
            .into_iter()
            .map(|path| TextureAsset {
                path: path.into(),
                width: 1,
                height: 1,
                frames: 1,
                category: Category::Icon,
                tiled: false,
            })
            .collect();
        let mut tree = AssetTree::default();
        tree.update(&assets, &[(0, 0), (1, 0), (2, 0), (3, 0)]);
        let gfx = &tree.roots["gfx"];
        assert_eq!(
            gfx.children.keys().map(String::as_str).collect::<Vec<_>>(),
            ["a", "a.dds", "ab"]
        );
        assert_eq!(gfx.children["a"].children["z.dds"].asset, Some(0));
        assert_eq!(gfx.children["a.dds"].asset, Some(3));
        assert_eq!(gfx.children["ab"].children["x.dds"].asset, Some(1));
        tree.collapse_all();
        assert_eq!(tree.roots["gfx"].children["a"].open, Some(false));
        tree.expand_all();
        assert_eq!(
            tree.roots["gfx"].children["a"].children["b"].open,
            Some(true)
        );
        tree.update(&assets, &[(2, 0)]);
        let a = &tree.roots["gfx"].children["a"];
        assert_eq!(a.children["b"].children["y.dds"].path, "gfx/a/b/y.dds");
        assert_eq!(a.children["b"].children["y.dds"].asset, Some(2));
        assert_eq!(tree.roots["gfx"].children.len(), 1);
    }
}
