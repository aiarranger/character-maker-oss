// mapbox/earcut 3.0.1 の写し（three の extras/lib/earcut.js と同じ結果）。連結リストは配列の添字で持つ。座標は 2 次元。
const NONE: usize = usize::MAX;

#[derive(Clone)]
struct Node { i: usize, x: f64, y: f64, prev: usize, next: usize, z: i32, prev_z: usize, next_z: usize, steiner: bool }

struct E { n: Vec<Node> }

/// data は [x0, y0, x1, y1, ...]。戻りは 3 つずつの頂点番号
pub fn earcut(data: &[f64], hole_indices: &[usize]) -> Vec<usize> {
    let mut e = E { n: Vec::with_capacity(data.len() / 2 * 3 / 2) };
    let has_holes = !hole_indices.is_empty();
    let outer_len = if has_holes { hole_indices[0] * 2 } else { data.len() };
    let mut triangles = Vec::new();
    let mut outer = e.linked_list(data, 0, outer_len, true);
    if outer == NONE || e.n[outer].next == e.n[outer].prev { return triangles; }
    if has_holes { outer = e.eliminate_holes(data, hole_indices, outer); }

    let (mut min_x, mut min_y, mut inv_size) = (0.0f64, 0.0f64, 0.0f64);
    if data.len() > 80 * 2 {
        min_x = f64::INFINITY;
        min_y = f64::INFINITY;
        let (mut max_x, mut max_y) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        let mut i = 2;
        while i < outer_len {
            let (x, y) = (data[i], data[i + 1]);
            if x < min_x { min_x = x; }
            if y < min_y { min_y = y; }
            if x > max_x { max_x = x; }
            if y > max_y { max_y = y; }
            i += 2;
        }
        inv_size = (max_x - min_x).max(max_y - min_y);
        inv_size = if inv_size != 0.0 { 32767.0 / inv_size } else { 0.0 };
    }
    e.earcut_linked(outer, &mut triangles, min_x, min_y, inv_size, 0);
    triangles
}

fn signed_area(data: &[f64], start: usize, end: usize) -> f64 {
    let mut sum = 0.0;
    let (mut i, mut j) = (start, end.wrapping_sub(2));
    while i < end {
        sum += (data[j] - data[i]) * (data[i + 1] + data[j + 1]);
        j = i;
        i += 2;
    }
    sum
}

fn z_order(x: f64, y: f64, min_x: f64, min_y: f64, inv: f64) -> i32 {
    let mut x = ((x - min_x) * inv) as i32;
    let mut y = ((y - min_y) * inv) as i32;
    x = (x | (x << 8)) & 0x00FF00FF;
    x = (x | (x << 4)) & 0x0F0F0F0F;
    x = (x | (x << 2)) & 0x33333333;
    x = (x | (x << 1)) & 0x55555555;
    y = (y | (y << 8)) & 0x00FF00FF;
    y = (y | (y << 4)) & 0x0F0F0F0F;
    y = (y | (y << 2)) & 0x33333333;
    y = (y | (y << 1)) & 0x55555555;
    x | (y << 1)
}

fn point_in_triangle(ax: f64, ay: f64, bx: f64, by: f64, cx: f64, cy: f64, px: f64, py: f64) -> bool {
    (cx - px) * (ay - py) >= (ax - px) * (cy - py)
        && (ax - px) * (by - py) >= (bx - px) * (ay - py)
        && (bx - px) * (cy - py) >= (cx - px) * (by - py)
}
fn point_in_triangle_except_first(ax: f64, ay: f64, bx: f64, by: f64, cx: f64, cy: f64, px: f64, py: f64) -> bool {
    !(ax == px && ay == py) && point_in_triangle(ax, ay, bx, by, cx, cy, px, py)
}
fn sign(v: f64) -> i32 { if v > 0.0 { 1 } else if v < 0.0 { -1 } else { 0 } }

impl E {
    fn create_node(&mut self, i: usize, x: f64, y: f64) -> usize {
        self.n.push(Node { i, x, y, prev: NONE, next: NONE, z: 0, prev_z: NONE, next_z: NONE, steiner: false });
        self.n.len() - 1
    }
    fn insert_node(&mut self, i: usize, x: f64, y: f64, last: usize) -> usize {
        let p = self.create_node(i, x, y);
        if last == NONE {
            self.n[p].prev = p;
            self.n[p].next = p;
        } else {
            let ln = self.n[last].next;
            self.n[p].next = ln;
            self.n[p].prev = last;
            self.n[ln].prev = p;
            self.n[last].next = p;
        }
        p
    }
    fn remove_node(&mut self, p: usize) {
        let (next, prev, pz, nz) = { let m = &self.n[p]; (m.next, m.prev, m.prev_z, m.next_z) };
        self.n[next].prev = prev;
        self.n[prev].next = next;
        if pz != NONE { self.n[pz].next_z = nz; }
        if nz != NONE { self.n[nz].prev_z = pz; }
    }
    fn area(&self, p: usize, q: usize, r: usize) -> f64 {
        let (p, q, r) = (&self.n[p], &self.n[q], &self.n[r]);
        (q.y - p.y) * (r.x - q.x) - (q.x - p.x) * (r.y - q.y)
    }
    fn equals(&self, a: usize, b: usize) -> bool { self.n[a].x == self.n[b].x && self.n[a].y == self.n[b].y }

    fn linked_list(&mut self, data: &[f64], start: usize, end: usize, clockwise: bool) -> usize {
        let mut last = NONE;
        if clockwise == (signed_area(data, start, end) > 0.0) {
            let mut i = start;
            while i < end { last = self.insert_node(i / 2, data[i], data[i + 1], last); i += 2; }
        } else {
            let mut i = end;
            while i > start { i -= 2; last = self.insert_node(i / 2, data[i], data[i + 1], last); }
        }
        if last != NONE && self.equals(last, self.n[last].next) {
            self.remove_node(last);
            last = self.n[last].next;
        }
        last
    }

    fn filter_points(&mut self, start: usize, end: usize) -> usize {
        if start == NONE { return start; }
        let mut end = if end == NONE { start } else { end };
        let mut p = start;
        loop {
            let mut again = false;
            let (prev, next) = (self.n[p].prev, self.n[p].next);
            if !self.n[p].steiner && (self.equals(p, next) || self.area(prev, p, next) == 0.0) {
                self.remove_node(p);
                p = self.n[p].prev;
                end = p;
                if p == self.n[p].next { break; }
                again = true;
            } else {
                p = next;
            }
            if !(again || p != end) { break; }
        }
        end
    }

    fn earcut_linked(&mut self, ear: usize, tri: &mut Vec<usize>, min_x: f64, min_y: f64, inv: f64, pass: u8) {
        if ear == NONE { return; }
        let mut ear = ear;
        if pass == 0 && inv != 0.0 { self.index_curve(ear, min_x, min_y, inv); }
        let mut stop = ear;
        while self.n[ear].prev != self.n[ear].next {
            let (prev, next) = (self.n[ear].prev, self.n[ear].next);
            if if inv != 0.0 { self.is_ear_hashed(ear, min_x, min_y, inv) } else { self.is_ear(ear) } {
                tri.push(self.n[prev].i);
                tri.push(self.n[ear].i);
                tri.push(self.n[next].i);
                self.remove_node(ear);
                ear = self.n[next].next;
                stop = self.n[next].next;
                continue;
            }
            ear = next;
            if ear == stop {
                if pass == 0 {
                    let f = self.filter_points(ear, NONE);
                    self.earcut_linked(f, tri, min_x, min_y, inv, 1);
                } else if pass == 1 {
                    let f = self.filter_points(ear, NONE);
                    let c = self.cure_local_intersections(f, tri);
                    self.earcut_linked(c, tri, min_x, min_y, inv, 2);
                } else if pass == 2 {
                    self.split_earcut(ear, tri, min_x, min_y, inv);
                }
                break;
            }
        }
    }

    fn is_ear(&self, ear: usize) -> bool {
        let (a, b, c) = (self.n[ear].prev, ear, self.n[ear].next);
        if self.area(a, b, c) >= 0.0 { return false; }
        let (ax, bx, cx, ay, by, cy) = (self.n[a].x, self.n[b].x, self.n[c].x, self.n[a].y, self.n[b].y, self.n[c].y);
        let (x0, y0, x1, y1) = (ax.min(bx).min(cx), ay.min(by).min(cy), ax.max(bx).max(cx), ay.max(by).max(cy));
        let mut p = self.n[c].next;
        while p != a {
            let q = &self.n[p];
            if q.x >= x0 && q.x <= x1 && q.y >= y0 && q.y <= y1
                && point_in_triangle_except_first(ax, ay, bx, by, cx, cy, q.x, q.y)
                && self.area(q.prev, p, q.next) >= 0.0 { return false; }
            p = q.next;
        }
        true
    }

    fn is_ear_hashed(&self, ear: usize, min_x: f64, min_y: f64, inv: f64) -> bool {
        let (a, b, c) = (self.n[ear].prev, ear, self.n[ear].next);
        if self.area(a, b, c) >= 0.0 { return false; }
        let (ax, bx, cx, ay, by, cy) = (self.n[a].x, self.n[b].x, self.n[c].x, self.n[a].y, self.n[b].y, self.n[c].y);
        let (x0, y0, x1, y1) = (ax.min(bx).min(cx), ay.min(by).min(cy), ax.max(bx).max(cx), ay.max(by).max(cy));
        let min_z = z_order(x0, y0, min_x, min_y, inv);
        let max_z = z_order(x1, y1, min_x, min_y, inv);
        let (mut p, mut n) = (self.n[ear].prev_z, self.n[ear].next_z);
        let inside = |s: &E, k: usize| {
            let q = &s.n[k];
            q.x >= x0 && q.x <= x1 && q.y >= y0 && q.y <= y1 && k != a && k != c
                && point_in_triangle_except_first(ax, ay, bx, by, cx, cy, q.x, q.y)
                && s.area(q.prev, k, q.next) >= 0.0
        };
        while p != NONE && self.n[p].z >= min_z && n != NONE && self.n[n].z <= max_z {
            if inside(self, p) { return false; }
            p = self.n[p].prev_z;
            if inside(self, n) { return false; }
            n = self.n[n].next_z;
        }
        while p != NONE && self.n[p].z >= min_z {
            if inside(self, p) { return false; }
            p = self.n[p].prev_z;
        }
        while n != NONE && self.n[n].z <= max_z {
            if inside(self, n) { return false; }
            n = self.n[n].next_z;
        }
        true
    }

    fn cure_local_intersections(&mut self, start: usize, tri: &mut Vec<usize>) -> usize {
        let mut start = start;
        let mut p = start;
        loop {
            let a = self.n[p].prev;
            let pn = self.n[p].next;
            let b = self.n[pn].next;
            if !self.equals(a, b) && self.intersects(a, p, pn, b) && self.locally_inside(a, b) && self.locally_inside(b, a) {
                tri.push(self.n[a].i);
                tri.push(self.n[p].i);
                tri.push(self.n[b].i);
                self.remove_node(p);
                let pn2 = self.n[p].next;
                self.remove_node(pn2);
                start = b;
                p = b;
            }
            p = self.n[p].next;
            if p == start { break; }
        }
        self.filter_points(p, NONE)
    }

    fn split_earcut(&mut self, start: usize, tri: &mut Vec<usize>, min_x: f64, min_y: f64, inv: f64) {
        let mut a = start;
        loop {
            let mut b = self.n[self.n[a].next].next;
            while b != self.n[a].prev {
                if self.n[a].i != self.n[b].i && self.is_valid_diagonal(a, b) {
                    let c = self.split_polygon(a, b);
                    let an = self.n[a].next;
                    let a2 = self.filter_points(a, an);
                    let cn = self.n[c].next;
                    let c2 = self.filter_points(c, cn);
                    self.earcut_linked(a2, tri, min_x, min_y, inv, 0);
                    self.earcut_linked(c2, tri, min_x, min_y, inv, 0);
                    return;
                }
                b = self.n[b].next;
            }
            a = self.n[a].next;
            if a == start { break; }
        }
    }

    fn eliminate_holes(&mut self, data: &[f64], holes: &[usize], outer: usize) -> usize {
        let mut queue = Vec::new();
        for (i, &h) in holes.iter().enumerate() {
            let start = h * 2;
            let end = if i + 1 < holes.len() { holes[i + 1] * 2 } else { data.len() };
            let list = self.linked_list(data, start, end, false);
            if list == NONE { continue; }
            if list == self.n[list].next { self.n[list].steiner = true; }
            queue.push(self.get_leftmost(list));
        }
        queue.sort_by(|&a, &b| self.compare_xy_slope(a, b).partial_cmp(&0.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut outer = outer;
        for &h in &queue { outer = self.eliminate_hole(h, outer); }
        outer
    }

    fn compare_xy_slope(&self, a: usize, b: usize) -> f64 {
        let (na, nb) = (&self.n[a], &self.n[b]);
        let mut result = na.x - nb.x;
        if result == 0.0 {
            result = na.y - nb.y;
            if result == 0.0 {
                let (an, bn) = (&self.n[na.next], &self.n[nb.next]);
                result = (an.y - na.y) / (an.x - na.x) - (bn.y - nb.y) / (bn.x - nb.x);
            }
        }
        result
    }

    fn eliminate_hole(&mut self, hole: usize, outer: usize) -> usize {
        let bridge = self.find_hole_bridge(hole, outer);
        if bridge == NONE { return outer; }
        let rev = self.split_polygon(bridge, hole);
        let rn = self.n[rev].next;
        self.filter_points(rev, rn);
        let bn = self.n[bridge].next;
        self.filter_points(bridge, bn)
    }

    fn find_hole_bridge(&self, hole: usize, outer: usize) -> usize {
        let mut p = outer;
        let (hx, hy) = (self.n[hole].x, self.n[hole].y);
        let mut qx = f64::NEG_INFINITY;
        let mut m = NONE;
        if self.equals(hole, p) { return p; }
        loop {
            let pn = self.n[p].next;
            if self.equals(hole, pn) { return pn; }
            let (a, b) = (&self.n[p], &self.n[pn]);
            if hy <= a.y && hy >= b.y && b.y != a.y {
                let x = a.x + (hy - a.y) * (b.x - a.x) / (b.y - a.y);
                if x <= hx && x > qx {
                    qx = x;
                    m = if a.x < b.x { p } else { pn };
                    if x == hx { return m; }
                }
            }
            p = pn;
            if p == outer { break; }
        }
        if m == NONE { return NONE; }
        let stop = m;
        let (mx, my) = (self.n[m].x, self.n[m].y);
        let mut tan_min = f64::INFINITY;
        p = m;
        loop {
            let q = &self.n[p];
            let (ax, cx) = if hy < my { (hx, qx) } else { (qx, hx) };
            if hx >= q.x && q.x >= mx && hx != q.x && point_in_triangle(ax, hy, mx, my, cx, hy, q.x, q.y) {
                let tan = (hy - q.y).abs() / (hx - q.x);
                if self.locally_inside(p, hole)
                    && (tan < tan_min || (tan == tan_min && (q.x > self.n[m].x || (q.x == self.n[m].x && self.sector_contains_sector(m, p))))) {
                    m = p;
                    tan_min = tan;
                }
            }
            p = q.next;
            if p == stop { break; }
        }
        m
    }

    fn sector_contains_sector(&self, m: usize, p: usize) -> bool {
        self.area(self.n[m].prev, m, self.n[p].prev) < 0.0 && self.area(self.n[p].next, m, self.n[m].next) < 0.0
    }

    fn index_curve(&mut self, start: usize, min_x: f64, min_y: f64, inv: f64) {
        let mut p = start;
        loop {
            if self.n[p].z == 0 { self.n[p].z = z_order(self.n[p].x, self.n[p].y, min_x, min_y, inv); }
            self.n[p].prev_z = self.n[p].prev;
            self.n[p].next_z = self.n[p].next;
            p = self.n[p].next;
            if p == start { break; }
        }
        let pz = self.n[p].prev_z;
        self.n[pz].next_z = NONE;
        self.n[p].prev_z = NONE;
        self.sort_linked(p);
    }

    fn sort_linked(&mut self, list: usize) {
        let mut list = list;
        let mut in_size = 1;
        loop {
            let mut p = list;
            list = NONE;
            let mut tail = NONE;
            let mut num_merges = 0;
            while p != NONE {
                num_merges += 1;
                let mut q = p;
                let mut p_size = 0;
                for _ in 0..in_size {
                    p_size += 1;
                    q = self.n[q].next_z;
                    if q == NONE { break; }
                }
                let mut q_size = in_size;
                while p_size > 0 || (q_size > 0 && q != NONE) {
                    let e;
                    if p_size != 0 && (q_size == 0 || q == NONE || self.n[p].z <= self.n[q].z) {
                        e = p;
                        p = self.n[p].next_z;
                        p_size -= 1;
                    } else {
                        e = q;
                        q = self.n[q].next_z;
                        q_size -= 1;
                    }
                    if tail != NONE { self.n[tail].next_z = e; } else { list = e; }
                    self.n[e].prev_z = tail;
                    tail = e;
                }
                p = q;
            }
            self.n[tail].next_z = NONE;
            in_size *= 2;
            if num_merges <= 1 { break; }
        }
    }

    fn get_leftmost(&self, start: usize) -> usize {
        let (mut p, mut left) = (start, start);
        loop {
            let (q, l) = (&self.n[p], &self.n[left]);
            if q.x < l.x || (q.x == l.x && q.y < l.y) { left = p; }
            p = q.next;
            if p == start { break; }
        }
        left
    }

    fn is_valid_diagonal(&self, a: usize, b: usize) -> bool {
        let (na, nb) = (&self.n[a], &self.n[b]);
        self.n[na.next].i != nb.i && self.n[na.prev].i != nb.i && !self.intersects_polygon(a, b)
            && (self.locally_inside(a, b) && self.locally_inside(b, a) && self.middle_inside(a, b)
                && (self.area(na.prev, a, nb.prev) != 0.0 || self.area(a, nb.prev, b) != 0.0)
                || self.equals(a, b) && self.area(na.prev, a, na.next) > 0.0 && self.area(nb.prev, b, nb.next) > 0.0)
    }

    fn intersects(&self, p1: usize, q1: usize, p2: usize, q2: usize) -> bool {
        let o1 = sign(self.area(p1, q1, p2));
        let o2 = sign(self.area(p1, q1, q2));
        let o3 = sign(self.area(p2, q2, p1));
        let o4 = sign(self.area(p2, q2, q1));
        if o1 != o2 && o3 != o4 { return true; }
        if o1 == 0 && self.on_segment(p1, p2, q1) { return true; }
        if o2 == 0 && self.on_segment(p1, q2, q1) { return true; }
        if o3 == 0 && self.on_segment(p2, p1, q2) { return true; }
        if o4 == 0 && self.on_segment(p2, q1, q2) { return true; }
        false
    }
    fn on_segment(&self, p: usize, q: usize, r: usize) -> bool {
        let (p, q, r) = (&self.n[p], &self.n[q], &self.n[r]);
        q.x <= p.x.max(r.x) && q.x >= p.x.min(r.x) && q.y <= p.y.max(r.y) && q.y >= p.y.min(r.y)
    }
    fn intersects_polygon(&self, a: usize, b: usize) -> bool {
        let (ai, bi) = (self.n[a].i, self.n[b].i);
        let mut p = a;
        loop {
            let q = &self.n[p];
            let ni = self.n[q.next].i;
            if q.i != ai && ni != ai && q.i != bi && ni != bi && self.intersects(p, q.next, a, b) { return true; }
            p = q.next;
            if p == a { break; }
        }
        false
    }
    fn locally_inside(&self, a: usize, b: usize) -> bool {
        let (prev, next) = (self.n[a].prev, self.n[a].next);
        if self.area(prev, a, next) < 0.0 {
            self.area(a, b, next) >= 0.0 && self.area(a, prev, b) >= 0.0
        } else {
            self.area(a, b, prev) < 0.0 || self.area(a, next, b) < 0.0
        }
    }
    fn middle_inside(&self, a: usize, b: usize) -> bool {
        let mut p = a;
        let mut inside = false;
        let px = (self.n[a].x + self.n[b].x) / 2.0;
        let py = (self.n[a].y + self.n[b].y) / 2.0;
        loop {
            let (q, n) = (&self.n[p], &self.n[self.n[p].next]);
            if (q.y > py) != (n.y > py) && n.y != q.y && px < (n.x - q.x) * (py - q.y) / (n.y - q.y) + q.x { inside = !inside; }
            p = q.next;
            if p == a { break; }
        }
        inside
    }
    fn split_polygon(&mut self, a: usize, b: usize) -> usize {
        let a2 = self.create_node(self.n[a].i, self.n[a].x, self.n[a].y);
        let b2 = self.create_node(self.n[b].i, self.n[b].x, self.n[b].y);
        let an = self.n[a].next;
        let bp = self.n[b].prev;
        self.n[a].next = b;
        self.n[b].prev = a;
        self.n[a2].next = an;
        self.n[an].prev = a2;
        self.n[b2].next = a2;
        self.n[a2].prev = b2;
        self.n[bp].next = b2;
        self.n[b2].prev = bp;
        b2
    }
}
