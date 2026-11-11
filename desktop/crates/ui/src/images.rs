//! Decoded image cache with a byte budget. Off-screen images are dropped on the
//! following frame so GPU textures do not accumulate for scrolled-away output.
use gpui_kit::{
    AnyImageCache, App, AppContext, ElementId, Entity, ImageCache, ImageCacheError, ImageCacheItem,
    ImageCacheProvider, RenderImage, Resource, Window,
};
use std::{collections::HashMap, sync::Arc};

pub const DECODED_BUDGET: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Resident {
    pub key: u64,
    pub bytes: u64,
    pub seen: u64,
    pub used: u64,
}

pub(crate) fn stale_keys(residents: &[Resident], generation: u64) -> Vec<u64> {
    residents
        .iter()
        .filter(|resident| resident.seen + 1 < generation)
        .map(|resident| resident.key)
        .collect()
}

pub(crate) fn over_budget_keys(
    residents: &[Resident],
    generation: u64,
    budget: u64,
    protect: Option<u64>,
) -> Vec<u64> {
    let mut ranked: Vec<&Resident> = residents
        .iter()
        .filter(|resident| protect != Some(resident.key))
        .collect();
    ranked.sort_by(|left, right| {
        (right.seen < generation)
            .cmp(&(left.seen < generation))
            .then(left.used.cmp(&right.used))
            .then(left.key.cmp(&right.key))
    });
    let mut kept = residents.iter().map(|resident| resident.bytes).sum::<u64>();
    let mut dropped = Vec::new();
    for resident in ranked {
        if kept <= budget {
            break;
        }
        dropped.push(resident.key);
        kept = kept.saturating_sub(resident.bytes);
    }
    dropped
}

struct Slot {
    item: ImageCacheItem,
    bytes: u64,
    seen: u64,
    used: u64,
    image: Option<Arc<RenderImage>>,
}

pub struct BoundedImageCache {
    entries: HashMap<u64, Slot>,
    budget: u64,
    generation: u64,
    clock: u64,
    cleanup_scheduled: bool,
}

impl BoundedImageCache {
    fn new(budget: u64, cx: &mut App) -> Entity<Self> {
        let entity = cx.new(|_| Self {
            entries: HashMap::new(),
            budget,
            generation: 0,
            clock: 0,
            cleanup_scheduled: false,
        });
        cx.observe_release(&entity, |cache, cx| {
            for (_, slot) in std::mem::take(&mut cache.entries) {
                if let Some(image) = slot.image {
                    cx.drop_image(image, None);
                }
            }
        })
        .detach();
        entity
    }

    fn residents(&self) -> Vec<Resident> {
        self.entries
            .iter()
            .map(|(key, slot)| Resident {
                key: *key,
                bytes: slot.bytes,
                seen: slot.seen,
                used: slot.used,
            })
            .collect()
    }

    fn take(&mut self, keys: &[u64]) -> Vec<Arc<RenderImage>> {
        let mut images = Vec::new();
        for key in keys {
            if let Some(slot) = self.entries.remove(key)
                && let Some(image) = slot.image
            {
                images.push(image);
            }
        }
        images
    }

    fn begin_frame(&mut self) -> Vec<Arc<RenderImage>> {
        self.cleanup_scheduled = false;
        self.generation += 1;
        let stale = stale_keys(&self.residents(), self.generation);
        self.take(&stale)
    }

    fn remember(&mut self, image: &Arc<RenderImage>, key: u64) {
        let Some(slot) = self.entries.get_mut(&key) else {
            return;
        };
        let bytes = (0..image.frame_count())
            .filter_map(|frame| image.as_bytes(frame))
            .map(|frame| frame.len() as u64)
            .sum();
        slot.bytes = bytes;
        slot.image = Some(image.clone());
    }
}

impl ImageCache for BoundedImageCache {
    fn load(
        &mut self,
        resource: &Resource,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        let key = gpui_kit::hash(resource);
        self.clock += 1;
        let clock = self.clock;
        let generation = self.generation;
        let slot = self.entries.entry(key).or_insert_with(|| Slot {
            item: ImageCacheItem::new(resource, cx),
            bytes: 0,
            seen: generation,
            used: clock,
            image: None,
        });
        slot.seen = generation;
        slot.used = clock;
        let result = slot.item.use_image(window);
        if let Some(Ok(image)) = &result {
            let image = image.clone();
            self.remember(&image, key);
        }
        let dropped = {
            let keys = over_budget_keys(&self.residents(), self.generation, self.budget, Some(key));
            self.take(&keys)
        };
        for image in dropped {
            cx.drop_image(image, Some(window));
        }
        result
    }
}

pub struct BoundedImageCacheProvider {
    id: ElementId,
    budget: u64,
}

pub fn workspace_cache(id: impl Into<ElementId>) -> BoundedImageCacheProvider {
    BoundedImageCacheProvider {
        id: id.into(),
        budget: DECODED_BUDGET,
    }
}

impl ImageCacheProvider for BoundedImageCacheProvider {
    fn provide(&mut self, window: &mut Window, cx: &mut App) -> AnyImageCache {
        let budget = self.budget;
        let entity = window.with_global_id(self.id.clone(), |global_id, window| {
            window.with_element_state::<Entity<BoundedImageCache>, _>(global_id, |existing, _| {
                let entity = existing.unwrap_or_else(|| BoundedImageCache::new(budget, cx));
                (entity.clone(), entity)
            })
        });
        // GPUI asks for this provider during both layout and paint. Advancing
        // here would age an image twice per frame and cancel pending loads.
        let schedule = entity.update(cx, |cache, _| {
            let schedule = !cache.cleanup_scheduled && !cache.entries.is_empty();
            cache.cleanup_scheduled |= schedule;
            schedule
        });
        if schedule {
            let cache = entity.downgrade();
            window.on_next_frame(move |window, cx| {
                let _ = cache.update(cx, |cache, cx| {
                    for image in cache.begin_frame() {
                        cx.drop_image(image, Some(window));
                    }
                });
            });
        }
        entity.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{
        Context, InteractiveElement, IntoElement, ParentElement, Render, TestAppContext,
        WindowOptions, img,
        test::{TestSupportExt, TestWindowExt},
    };

    struct TestImage(std::path::PathBuf);

    impl Render for TestImage {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            gpui_kit::image_cache(workspace_cache("test-images"))
                .child(img(self.0.clone()).id("loaded-image").test_support())
        }
    }

    #[gpui_kit::test]
    fn image_load_survives_layout_and_paint_cache_requests(cx: &mut TestAppContext) {
        let path = std::env::temp_dir().join(format!("kl-cache-{}.png", uuid::Uuid::new_v4()));
        // A real 2×2 PNG, decoded asynchronously by the same loader as notebook output.
        std::fs::write(
            &path,
            [
                137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 2, 0, 0, 0,
                2, 8, 2, 0, 0, 0, 253, 212, 154, 115, 0, 0, 0, 22, 73, 68, 65, 84, 120, 156, 99,
                252, 207, 192, 192, 192, 192, 192, 196, 192, 192, 192, 192, 192, 0, 0, 13, 29, 1,
                3, 106, 194, 155, 233, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
            ],
        )
        .unwrap();
        let (handle, _) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                cx.new(|_| TestImage(path.clone()))
            })
            .unwrap()
        });
        for _ in 0..3 {
            cx.update_window(handle, |_, window, cx| {
                window.refresh();
                window.render_frame(cx);
                window.simulate_next_frame(cx);
            })
            .unwrap();
            cx.run_until_parked();
        }
        cx.update_window(handle, |_, window, _| {
            assert_eq!(
                window.find("loaded-image").bounds().size,
                gpui_kit::size(gpui_kit::px(2.), gpui_kit::px(2.))
            );
        })
        .unwrap();
        std::fs::remove_file(path).unwrap();
    }

    fn resident(key: u64, bytes: u64, seen: u64, used: u64) -> Resident {
        Resident {
            key,
            bytes,
            seen,
            used,
        }
    }

    #[test]
    fn offscreen_images_leave_on_the_following_frame() {
        let residents = vec![resident(1, 10, 1, 1), resident(2, 10, 2, 2)];
        assert!(stale_keys(&residents, 2).is_empty());
        assert_eq!(stale_keys(&residents, 3), vec![1]);
    }

    #[test]
    fn budget_evicts_offscreen_images_before_the_one_just_shown() {
        let residents = vec![resident(1, 40, 1, 1), resident(2, 40, 2, 2)];
        assert_eq!(over_budget_keys(&residents, 2, 50, Some(2)), vec![1]);
        assert!(over_budget_keys(&residents, 2, 80, Some(2)).is_empty());
        assert_eq!(
            over_budget_keys(
                &[resident(1, 40, 2, 1), resident(2, 40, 2, 2)],
                2,
                50,
                Some(2)
            ),
            vec![1]
        );
    }
}
