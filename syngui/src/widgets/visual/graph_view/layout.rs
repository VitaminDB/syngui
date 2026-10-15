//! Послойная раскладка ориентированного графа (в духе Sugiyama) для графов
//! потока управления: корень сверху, переходы вниз, циклы — обратными рёбрами
//! в обход справа. Чистая геометрия, без виджетов.
//!
//! 1. Обход в глубину от корня отмечает обратные рёбра (к узлу на стеке).
//! 2. Слой узла — длина самого длинного пути от истоков по прямым рёбрам.
//! 3. Порядок в слое — по барицентрам соседей (несколько проходов вниз/вверх).
//! 4. X — под центром предков, без наложений; Y — по высотам слоёв.
//! 5. Рёбра — ортогональные ломаные: вниз, вбок по «коридору» между слоями,
//!    вниз; обратные — вниз, вправо за граф, вверх, влево, вниз в цель.

/// Итог раскладки.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    /// Левый верхний угол каждого узла.
    pub pos: Vec<(f32, f32)>,
    /// Ломаная каждого ребра (в порядке входа), от источника к цели.
    pub routes: Vec<Vec<(f32, f32)>>,
    /// Обратное ли ребро (цикл).
    pub back: Vec<bool>,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Spacing {
    pub gap_x: f32,
    pub gap_y: f32,
}

impl Default for Spacing {
    fn default() -> Self {
        Self { gap_x: 36.0, gap_y: 56.0 }
    }
}

/// Разложить граф: `sizes[i]` — (ширина, высота) узла, `edges` — (из, в).
pub fn layout(sizes: &[(f32, f32)], edges: &[(usize, usize)], root: usize, sp: Spacing) -> Layout {
    let n = sizes.len();
    if n == 0 {
        return Layout::default();
    }
    let mut out_adj = vec![Vec::new(); n];
    for (i, &(a, b)) in edges.iter().enumerate() {
        if a < n && b < n {
            out_adj[a].push((b, i));
        }
    }

    // 1. обратные рёбра — DFS (итеративный) от корня, затем от непосещённых
    let mut back = vec![false; edges.len()];
    let mut state = vec![0u8; n]; // 0 — не был, 1 — на стеке, 2 — готов
    let mut order_seen = Vec::with_capacity(n);
    let starts = std::iter::once(root.min(n - 1)).chain(0..n);
    for s in starts {
        if state[s] != 0 {
            continue;
        }
        let mut stack: Vec<(usize, usize)> = vec![(s, 0)];
        state[s] = 1;
        order_seen.push(s);
        while let Some(&mut (v, ref mut k)) = stack.last_mut() {
            if *k < out_adj[v].len() {
                let (w, ei) = out_adj[v][*k];
                *k += 1;
                match state[w] {
                    0 => {
                        state[w] = 1;
                        order_seen.push(w);
                        stack.push((w, 0));
                    }
                    1 => back[ei] = true,
                    _ => {}
                }
            } else {
                state[v] = 2;
                stack.pop();
            }
        }
    }

    // 2. слои: самый длинный путь по прямым рёбрам (Кан)
    let mut indeg = vec![0usize; n];
    for (i, &(a, b)) in edges.iter().enumerate() {
        if !back[i] && a < n && b < n && a != b {
            indeg[b] += 1;
        }
    }
    let mut layer = vec![0usize; n];
    let mut queue: std::collections::VecDeque<usize> = (0..n).filter(|&v| indeg[v] == 0).collect();
    while let Some(v) = queue.pop_front() {
        for &(w, ei) in &out_adj[v] {
            if back[ei] || w == v {
                continue;
            }
            layer[w] = layer[w].max(layer[v] + 1);
            indeg[w] -= 1;
            if indeg[w] == 0 {
                queue.push_back(w);
            }
        }
    }
    let layers_n = layer.iter().max().copied().unwrap_or(0) + 1;
    let mut layers: Vec<Vec<usize>> = vec![Vec::new(); layers_n];
    for &v in &order_seen {
        layers[layer[v]].push(v);
    }

    // 3. порядок по барицентрам
    let mut preds = vec![Vec::new(); n];
    let mut succs = vec![Vec::new(); n];
    for (i, &(a, b)) in edges.iter().enumerate() {
        if !back[i] && a < n && b < n && a != b {
            succs[a].push(b);
            preds[b].push(a);
        }
    }
    let mut idx = vec![0f32; n];
    let renumber = |layers: &Vec<Vec<usize>>, idx: &mut Vec<f32>| {
        for l in layers {
            for (k, &v) in l.iter().enumerate() {
                idx[v] = k as f32;
            }
        }
    };
    renumber(&layers, &mut idx);
    for pass in 0..6 {
        let down = pass % 2 == 0;
        let range: Vec<usize> = if down { (1..layers_n).collect() } else { (0..layers_n.saturating_sub(1)).rev().collect() };
        for li in range {
            let mut keyed: Vec<(f32, usize)> = layers[li]
                .iter()
                .map(|&v| {
                    let nb = if down { &preds[v] } else { &succs[v] };
                    let bc = if nb.is_empty() { idx[v] } else { nb.iter().map(|&u| idx[u]).sum::<f32>() / nb.len() as f32 };
                    (bc, v)
                })
                .collect();
            keyed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            layers[li] = keyed.into_iter().map(|(_, v)| v).collect();
            for (k, &v) in layers[li].iter().enumerate() {
                idx[v] = k as f32;
            }
        }
    }

    // 4. координаты
    let mut layer_y = vec![0f32; layers_n];
    let mut y = 0.0;
    for (li, l) in layers.iter().enumerate() {
        layer_y[li] = y;
        let h = l.iter().map(|&v| sizes[v].1).fold(0.0, f32::max);
        y += h + sp.gap_y;
    }
    let mut x = vec![0f32; n];
    for l in &layers {
        let mut cur = 0.0;
        for &v in l {
            x[v] = cur;
            cur += sizes[v].0 + sp.gap_x;
        }
    }
    let center = |x: &Vec<f32>, v: usize| x[v] + sizes[v].0 / 2.0;
    for _ in 0..3 {
        for li in 1..layers_n {
            // желаемое — центр под предками; затем раздвинуть без наложений
            let l = layers[li].clone();
            let mut want: Vec<f32> =
                l.iter().map(|&v| if preds[v].is_empty() { x[v] } else { preds[v].iter().map(|&u| center(&x, u)).sum::<f32>() / preds[v].len() as f32 - sizes[v].0 / 2.0 }).collect();
            for k in 1..l.len() {
                let min = want[k - 1] + sizes[l[k - 1]].0 + sp.gap_x;
                if want[k] < min {
                    want[k] = min;
                }
            }
            for (k, &v) in l.iter().enumerate() {
                x[v] = want[k];
            }
        }
        // корень и истоки — над центром своих потомков
        for li in (0..layers_n.saturating_sub(1)).rev() {
            let l = layers[li].clone();
            let mut want: Vec<f32> =
                l.iter().map(|&v| if succs[v].is_empty() { x[v] } else { succs[v].iter().map(|&u| center(&x, u)).sum::<f32>() / succs[v].len() as f32 - sizes[v].0 / 2.0 }).collect();
            for k in 1..l.len() {
                let min = want[k - 1] + sizes[l[k - 1]].0 + sp.gap_x;
                if want[k] < min {
                    want[k] = min;
                }
            }
            for (k, &v) in l.iter().enumerate() {
                x[v] = want[k];
            }
        }
    }
    let min_x = x.iter().copied().fold(f32::MAX, f32::min);
    for v in x.iter_mut() {
        *v -= min_x;
    }
    let width = (0..n).map(|v| x[v] + sizes[v].0).fold(0.0, f32::max);
    let pos: Vec<(f32, f32)> = (0..n).map(|v| (x[v], layer_y[layer[v]])).collect();

    // 5. рёбра: выходы/входы разнесены по ширине узла
    let mut out_k = vec![0usize; n];
    let mut in_k = vec![0usize; n];
    let out_cnt: Vec<usize> = (0..n).map(|v| out_adj[v].len()).collect();
    let mut in_cnt = vec![0usize; n];
    for &(_, b) in edges {
        if b < n {
            in_cnt[b] += 1;
        }
    }
    let port = |v: usize, k: usize, cnt: usize| pos[v].0 + sizes[v].0 * (k + 1) as f32 / (cnt + 1) as f32;
    let mut back_lane = 0f32;
    let mut lane_in_gap = vec![0usize; layers_n];
    let mut routes = Vec::with_capacity(edges.len());
    for (i, &(a, b)) in edges.iter().enumerate() {
        if a >= n || b >= n {
            routes.push(Vec::new());
            continue;
        }
        let sx = port(a, out_k[a], out_cnt[a]);
        out_k[a] += 1;
        let tx = port(b, in_k[b], in_cnt[b]);
        in_k[b] += 1;
        let sy = pos[a].1 + sizes[a].1;
        let ty = pos[b].1;
        let la = layer[a];
        // свой «коридор» по высоте для каждого ребра из слоя — меньше слипания
        let lane = lane_in_gap[la];
        lane_in_gap[la] += 1;
        let lane_off = 10.0 + (lane % 6) as f32 * ((sp.gap_y - 20.0) / 6.0);
        let layer_bottom = layer_y[la] + layers[la].iter().map(|&v| sizes[v].1).fold(0.0, f32::max);
        if !back[i] {
            let mid = layer_bottom + lane_off.min(sp.gap_y - 8.0);
            if (sx - tx).abs() < 0.5 {
                routes.push(vec![(sx, sy), (tx, ty)]);
            } else {
                routes.push(vec![(sx, sy), (sx, mid), (tx, mid), (tx, ty)]);
            }
        } else {
            back_lane += 12.0;
            let right = width + 16.0 + back_lane;
            let below = layer_bottom + lane_off.min(sp.gap_y - 8.0);
            let above = ty - 12.0 - (back_lane % 30.0);
            routes.push(vec![(sx, sy), (sx, below), (right, below), (right, above), (tx, above), (tx, ty)]);
        }
    }
    let width = width + 16.0 + back_lane + 12.0;
    let height = y - sp.gap_y;
    Layout { pos, routes, back, width, height }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlap(l: &Layout, sizes: &[(f32, f32)]) -> bool {
        for i in 0..sizes.len() {
            for j in i + 1..sizes.len() {
                let (a, b) = (l.pos[i], l.pos[j]);
                let sep = a.0 + sizes[i].0 <= b.0 || b.0 + sizes[j].0 <= a.0 || a.1 + sizes[i].1 <= b.1 || b.1 + sizes[j].1 <= a.1;
                if !sep {
                    return true;
                }
            }
        }
        false
    }

    #[test]
    fn diamond_with_loop() {
        // 0 → 1, 0 → 2, 1 → 3, 2 → 3, 3 → 0 (цикл)
        let sizes = vec![(100.0, 40.0), (80.0, 60.0), (120.0, 30.0), (90.0, 50.0)];
        let edges = vec![(0, 1), (0, 2), (1, 3), (2, 3), (3, 0)];
        let l = layout(&sizes, &edges, 0, Spacing::default());
        assert!(!overlap(&l, &sizes));
        assert_eq!(l.back, [false, false, false, false, true]);
        // слои: 0 сверху, 1 и 2 на одном уровне, 3 ниже
        assert!(l.pos[0].1 < l.pos[1].1);
        assert_eq!(l.pos[1].1, l.pos[2].1);
        assert!(l.pos[3].1 > l.pos[1].1);
        // ортогональные ломаные: соседние точки на одной вертикали или горизонтали
        for r in &l.routes {
            for w in r.windows(2) {
                assert!((w[0].0 - w[1].0).abs() < 0.01 || (w[0].1 - w[1].1).abs() < 0.01, "{r:?}");
            }
        }
        // обратное ребро уходит правее графа
        assert!(l.routes[4].iter().any(|p| p.0 > l.pos.iter().zip(&sizes).map(|(p, s)| p.0 + s.0).fold(0.0, f32::max)));
    }

    #[test]
    fn chain_and_isolated() {
        let sizes = vec![(50.0, 20.0); 5];
        let edges = vec![(0, 1), (1, 2), (2, 2)];
        let l = layout(&sizes, &edges, 0, Spacing::default());
        assert!(!overlap(&l, &sizes));
        assert!(l.pos[0].1 < l.pos[1].1 && l.pos[1].1 < l.pos[2].1);
        assert!(l.back[2], "петля на себя — обратное ребро");
        assert!(layout(&[], &[], 0, Spacing::default()).pos.is_empty());
    }
}
