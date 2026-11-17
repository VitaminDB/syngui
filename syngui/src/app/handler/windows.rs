//! Липкие окна (`AppBuilder::sticky_windows`): прилипание краями и
//! перемещение группой.

use super::AppHandler;

impl AppHandler {
    pub(in crate::app) fn handle_window_moved(
        &mut self,
        moved_id: winit::window::WindowId,
        new_x: i32,
        new_y: i32,
    ) {
        let threshold = match self.sticky_threshold {
            Some(t) => t as i32,
            None => return,
        };

        let old_pos = self.window_positions.get(&moved_id).copied();
        self.window_positions.insert(moved_id, (new_x, new_y));

        let mut all_windows: Vec<(winit::window::WindowId, (i32, i32), (i32, i32))> = Vec::new();
        if let Some(ref win) = self.win.window {
            let id = win.winit_window().id();
            let (w, h) = win.size();
            if let Ok(pos) = win.winit_window().outer_position() {
                self.window_positions.insert(id, (pos.x, pos.y));
                all_windows.push((id, (pos.x, pos.y), (w as i32, h as i32)));
            }
        }
        for (&id, sw) in &self.secondary_windows {
            let (w, h) = (sw.width as i32, sw.height as i32);
            let Some(window) = sw.window.as_ref() else {
                continue;
            };
            if let Ok(pos) = window.winit_window().outer_position() {
                self.window_positions.insert(id, (pos.x, pos.y));
                all_windows.push((id, (pos.x, pos.y), (w, h)));
            }
        }

        let moved_idx = all_windows.iter().position(|(id, _, _)| *id == moved_id);
        if moved_idx.is_none() {
            return;
        }
        let moved_idx = moved_idx.unwrap();
        let (_, moved_pos, moved_size) = all_windows[moved_idx];

        let delta = if let Some((ox, oy)) = old_pos {
            (new_x - ox, new_y - oy)
        } else {
            (0, 0)
        };

        if delta.0 != 0 || delta.1 != 0 {
            let group = self
                .sticky_groups
                .iter()
                .find(|g| g.contains(&moved_id))
                .cloned();
            if let Some(group) = group {
                for &gid in &group {
                    if gid == moved_id {
                        continue;
                    }
                    if let Some(&(gx, gy)) = self.window_positions.get(&gid) {
                        let new_gx = gx + delta.0;
                        let new_gy = gy + delta.1;
                        self.set_window_position(gid, new_gx, new_gy);
                        self.window_positions.insert(gid, (new_gx, new_gy));
                    }
                }
            }
        }

        let mut snap_x = moved_pos.0;
        let mut snap_y = moved_pos.1;
        let mut snapped_to: Option<winit::window::WindowId> = None;

        for (i, &(other_id, other_pos, other_size)) in all_windows.iter().enumerate() {
            if i == moved_idx {
                continue;
            }

            let r2l = (moved_pos.0 + moved_size.0 - other_pos.0).abs();
            let l2r = (moved_pos.0 - (other_pos.0 + other_size.0)).abs();
            let b2t = (moved_pos.1 + moved_size.1 - other_pos.1).abs();
            let t2b = (moved_pos.1 - (other_pos.1 + other_size.1)).abs();

            let v_overlap = moved_pos.1 < other_pos.1 + other_size.1
                && moved_pos.1 + moved_size.1 > other_pos.1;
            let h_overlap = moved_pos.0 < other_pos.0 + other_size.0
                && moved_pos.0 + moved_size.0 > other_pos.0;

            if v_overlap {
                if r2l < threshold {
                    snap_x = other_pos.0 - moved_size.0;
                    snapped_to = Some(other_id);
                }
                if l2r < threshold {
                    snap_x = other_pos.0 + other_size.0;
                    snapped_to = Some(other_id);
                }
            }
            if h_overlap {
                if b2t < threshold {
                    snap_y = other_pos.1 - moved_size.1;
                    snapped_to = Some(other_id);
                }
                if t2b < threshold {
                    snap_y = other_pos.1 + other_size.1;
                    snapped_to = Some(other_id);
                }
            }

            if v_overlap && (r2l < threshold || l2r < threshold) {
                if (moved_pos.1 - other_pos.1).abs() < threshold {
                    snap_y = other_pos.1;
                }
            }
            if h_overlap && (b2t < threshold || t2b < threshold) {
                if (moved_pos.0 - other_pos.0).abs() < threshold {
                    snap_x = other_pos.0;
                }
            }
        }

        if snap_x != moved_pos.0 || snap_y != moved_pos.1 {
            self.set_window_position(moved_id, snap_x, snap_y);
            self.window_positions.insert(moved_id, (snap_x, snap_y));

            if let Some(other_id) = snapped_to {
                self.merge_sticky_group(moved_id, other_id);
            }
        } else {
            let detach_threshold = threshold * 3;
            let should_detach =
                if let Some(group) = self.sticky_groups.iter().find(|g| g.contains(&moved_id)) {
                    group.iter().filter(|&&gid| gid != moved_id).all(|&gid| {
                        if let Some(&(gx, gy)) = self.window_positions.get(&gid) {
                            let dx = (moved_pos.0 - gx).abs();
                            let dy = (moved_pos.1 - gy).abs();
                            dx > detach_threshold * 3 || dy > detach_threshold * 3
                        } else {
                            true
                        }
                    })
                } else {
                    false
                };
            if should_detach {
                self.remove_from_sticky_group(moved_id);
            }
        }
    }

    fn set_window_position(&self, id: winit::window::WindowId, x: i32, y: i32) {
        if let Some(ref win) = self.win.window {
            if win.winit_window().id() == id {
                win.winit_window()
                    .set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
                return;
            }
        }
        if let Some(window) = self
            .secondary_windows
            .get(&id)
            .and_then(|sw| sw.window.as_ref())
        {
            window
                .winit_window()
                .set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
        }
    }

    fn merge_sticky_group(&mut self, a: winit::window::WindowId, b: winit::window::WindowId) {
        let group_a = self.sticky_groups.iter().position(|g| g.contains(&a));
        let group_b = self.sticky_groups.iter().position(|g| g.contains(&b));

        match (group_a, group_b) {
            (Some(ga), Some(gb)) if ga != gb => {
                let group_b_set = self.sticky_groups.remove(gb.max(ga));
                let ga_idx = if gb < ga { ga - 1 } else { ga };
                let group_a_set = self.sticky_groups.remove(ga_idx.min(gb));
                let mut merged = group_a_set;
                merged.extend(group_b_set);
                self.sticky_groups.push(merged);
            }
            (Some(_), Some(_)) => {}
            (Some(ga), None) => {
                self.sticky_groups[ga].insert(b);
            }
            (None, Some(gb)) => {
                self.sticky_groups[gb].insert(a);
            }
            (None, None) => {
                let mut group = std::collections::HashSet::new();
                group.insert(a);
                group.insert(b);
                self.sticky_groups.push(group);
            }
        }
    }

    pub(super) fn remove_from_sticky_group(&mut self, id: winit::window::WindowId) {
        for group in &mut self.sticky_groups {
            group.remove(&id);
        }
        self.sticky_groups.retain(|g| g.len() > 1);
    }
}
