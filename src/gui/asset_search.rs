use std::cmp::Reverse;

use nucleo_matcher::{
    Config, Matcher, Utf32Str,
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
};

use crate::{compiler::TextureAsset, config::Category};

pub(super) struct AssetSearch {
    matcher: Matcher,
    buffer: Vec<char>,
    pub(super) matches: Vec<(usize, u32)>,
}

impl AssetSearch {
    pub(super) fn update(
        &mut self,
        assets: &[TextureAsset],
        query: &str,
        category: Option<Category>,
    ) {
        let pattern = Pattern::new(
            query,
            CaseMatching::Ignore,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );
        self.matches.clear();
        self.matches.extend(
            assets
                .iter()
                .enumerate()
                .filter(|(_, asset)| category.is_none_or(|category| category == asset.category))
                .filter_map(|(index, asset)| {
                    pattern
                        .score(
                            Utf32Str::new(&asset.path, &mut self.buffer),
                            &mut self.matcher,
                        )
                        .map(|score| (index, score))
                }),
        );
        self.matches
            .sort_unstable_by_key(|&(index, score)| (Reverse(score), index));
    }
}

impl Default for AssetSearch {
    fn default() -> Self {
        Self {
            matcher: Matcher::new(Config::DEFAULT.match_paths()),
            buffer: Vec::new(),
            matches: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(path: &str, category: Category) -> TextureAsset {
        TextureAsset {
            path: path.into(),
            width: 1,
            height: 1,
            frames: 1,
            category,
            tiled: false,
        }
    }

    #[test]
    fn fuzzy_search_ranks_matches_and_ignores_case() {
        let assets = [
            asset("gfx/interface/ship_buttons.dds", Category::Icon),
            asset("gfx/interface/sbt.dds", Category::Icon),
            asset("gfx/interface/planet.dds", Category::Icon),
        ];
        let mut search = AssetSearch::default();
        search.update(&assets, "SBT", None);
        assert_eq!(
            search.matches.iter().map(|&(i, _)| i).collect::<Vec<_>>(),
            [1, 0]
        );
    }

    #[test]
    fn words_and_category_filter_work_together() {
        let assets = [
            asset("gfx/interface/ship_buttons.dds", Category::Icon),
            asset("gfx/interface/ship_background.dds", Category::Background),
        ];
        let mut search = AssetSearch::default();
        search.update(&assets, "btn ship", Some(Category::Icon));
        assert_eq!(search.matches.len(), 1);
        assert_eq!(search.matches[0].0, 0);
        search.update(&assets, "ship", Some(Category::Background));
        assert_eq!(search.matches.len(), 1);
        assert_eq!(search.matches[0].0, 1);
    }

    #[test]
    fn empty_query_preserves_order_and_updates_clear_stale_results() {
        let assets = [
            asset("gfx/interface/z.dds", Category::Icon),
            asset("gfx/interface/a.dds", Category::Background),
        ];
        let mut search = AssetSearch::default();
        search.update(&assets, "  ", None);
        assert_eq!(search.matches, [(0, 0), (1, 0)]);
        search.update(&assets, "", Some(Category::Background));
        assert_eq!(search.matches, [(1, 0)]);
        search.update(&assets, "missing", None);
        assert_eq!(search.matches, []);
        search.update(&[], "", None);
        assert_eq!(search.matches, []);
    }
}
