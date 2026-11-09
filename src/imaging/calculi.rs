//! Search for small dense foci in a CT volume (urinary and biliary stones,
//! appendicoliths, calcifications): connected groups of voxels above a
//! threshold that are too small to be bone or contrast-filled vessels.

use super::volume::{Volume, OUTSIDE};
use glam::DVec3;

#[derive(Clone, Debug)]
pub struct Candidate {
    pub center: DVec3,
    pub volume_mm3: f64,
    /// Largest extent along the volume axes (mm).
    pub length_mm: f64,
    pub max_hu: f32,
    pub mean_hu: f32,
}

/// Accumulated data of one connected group.
#[derive(Clone, Copy)]
struct Stats {
    count: u64,
    sum: [f64; 3],
    sum_v: f64,
    max_v: f32,
    lo: [u32; 3],
    hi: [u32; 3],
}

impl Stats {
    fn merge(&mut self, o: &Stats) {
        self.count += o.count;
        for a in 0..3 {
            self.sum[a] += o.sum[a];
            self.lo[a] = self.lo[a].min(o.lo[a]);
            self.hi[a] = self.hi[a].max(o.hi[a]);
        }
        self.sum_v += o.sum_v;
        self.max_v = self.max_v.max(o.max_v);
    }
}

struct Run {
    x0: u32,
    x1: u32,
    node: u32,
}

fn find_root(parent: &mut [u32], mut a: u32) -> u32 {
    while parent[a as usize] != a {
        let p = parent[a as usize];
        parent[a as usize] = parent[p as usize];
        a = p;
    }
    a
}

fn union(parent: &mut [u32], stats: &mut [Stats], a: u32, b: u32) {
    let (ra, rb) = (find_root(parent, a), find_root(parent, b));
    if ra != rb {
        let (keep, gone) = if stats[ra as usize].count >= stats[rb as usize].count { (ra, rb) } else { (rb, ra) };
        parent[gone as usize] = keep;
        let s = stats[gone as usize];
        stats[keep as usize].merge(&s);
    }
}

/// Joins `runs` with the overlapping runs of a neighbouring row (6-connectivity).
fn link(parent: &mut [u32], stats: &mut [Stats], runs: &[Run], other: &[Run]) {
    let mut j = 0;
    for r in runs {
        while j < other.len() && other[j].x1 <= r.x0 {
            j += 1;
        }
        let mut k = j;
        while k < other.len() && other[k].x0 < r.x1 {
            union(parent, stats, r.node, other[k].node);
            k += 1;
        }
    }
}

/// Dense foci at or above `threshold_hu`, from about 1 mm³ up to
/// `max_volume_mm3`, densest first (at most 60).
pub fn find(vol: &Volume, threshold_hu: f32, max_volume_mm3: f64) -> Vec<Candidate> {
    let [nx, ny, nz] = vol.dims;
    let t = vol.quant.stored(threshold_hu);
    let mut parent: Vec<u32> = Vec::new();
    let mut stats: Vec<Stats> = Vec::new();
    let mut prev: Vec<Vec<Run>> = (0..ny).map(|_| Vec::new()).collect();
    let mut cur: Vec<Vec<Run>> = (0..ny).map(|_| Vec::new()).collect();
    for z in 0..nz {
        for y in 0..ny {
            let row = &vol.data[(z * ny + y) * nx..(z * ny + y + 1) * nx];
            let runs = &mut cur[y];
            runs.clear();
            let mut x = 0;
            while x < nx {
                let s = row[x];
                if s == OUTSIDE || (s as f32) < t {
                    x += 1;
                    continue;
                }
                let x0 = x;
                let mut st = Stats { count: 0, sum: [0.0; 3], sum_v: 0.0, max_v: f32::MIN, lo: [x0 as u32, y as u32, z as u32], hi: [0, y as u32, z as u32] };
                while x < nx && row[x] != OUTSIDE && (row[x] as f32) >= t {
                    let v = vol.quant.real(row[x]);
                    st.count += 1;
                    st.sum[0] += x as f64;
                    st.sum_v += v as f64;
                    st.max_v = st.max_v.max(v);
                    x += 1;
                }
                st.hi[0] = (x - 1) as u32;
                st.sum[1] = y as f64 * st.count as f64;
                st.sum[2] = z as f64 * st.count as f64;
                if parent.len() >= u32::MAX as usize - 1 {
                    return Vec::new();
                }
                let node = parent.len() as u32;
                parent.push(node);
                stats.push(st);
                runs.push(Run { x0: x0 as u32, x1: x as u32, node });
            }
            if y > 0 {
                let (above, here) = cur.split_at_mut(y);
                link(&mut parent, &mut stats, &here[0], &above[y - 1]);
            }
            link(&mut parent, &mut stats, &cur[y], &prev[y]);
        }
        std::mem::swap(&mut prev, &mut cur);
    }

    let voxel_mm3 = vol.spacing.x * vol.spacing.y * vol.spacing.z;
    let mut out: Vec<Candidate> = (0..parent.len() as u32)
        .filter(|&i| find_root(&mut parent, i) == i)
        .map(|i| stats[i as usize])
        .filter(|s| {
            let v = s.count as f64 * voxel_mm3;
            v >= 1.0_f64.max(voxel_mm3 * 2.0) && v <= max_volume_mm3
        })
        .map(|s| {
            let n = s.count as f64;
            let c = DVec3::new(s.sum[0] / n, s.sum[1] / n, s.sum[2] / n);
            let ext = DVec3::new(
                (s.hi[0] - s.lo[0] + 1) as f64 * vol.spacing.x,
                (s.hi[1] - s.lo[1] + 1) as f64 * vol.spacing.y,
                (s.hi[2] - s.lo[2] + 1) as f64 * vol.spacing.z,
            );
            Candidate {
                center: vol.voxel_to_patient(c),
                volume_mm3: n * voxel_mm3,
                length_mm: ext.max_element(),
                max_hu: s.max_v,
                mean_hu: (s.sum_v / n) as f32,
            }
        })
        .collect();
    out.sort_by(|a, b| b.max_hu.total_cmp(&a.max_hu));
    out.truncate(60);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::imaging::volume::Quant;

    #[test]
    fn finds_small_stones_not_bone() {
        let dims = [60, 50, 40];
        let mut data = vec![40i16; dims[0] * dims[1] * dims[2]];
        let idx = |x: usize, y: usize, z: usize| (z * dims[1] + y) * dims[0] + x;
        // A large "bone" plate.
        for z in 0..40 {
            for y in 40..48 {
                for x in 0..60 {
                    data[idx(x, y, z)] = 900;
                }
            }
        }
        // Two small stones (3×3×3 and 2×2×2 voxels) and one noisy single voxel.
        for (cx, cy, cz, r, v) in [(10, 10, 10, 3, 700i16), (40, 20, 25, 2, 450)] {
            for z in cz..cz + r {
                for y in cy..cy + r {
                    for x in cx..cx + r {
                        data[idx(x, y, z)] = v;
                    }
                }
            }
        }
        data[idx(30, 30, 30)] = 500;
        let vol = Volume::new(dims, DVec3::ONE, DVec3::ZERO, [DVec3::X, DVec3::Y, DVec3::Z], data, Quant::IDENTITY);
        let found = find(&vol, 200.0, 500.0);
        assert_eq!(found.len(), 2, "{found:?}");
        assert_eq!(found[0].max_hu, 700.0);
        assert!((found[0].center - DVec3::new(11.0, 11.0, 11.0)).length() < 1e-9);
        assert_eq!(found[0].volume_mm3, 27.0);
        assert_eq!(found[0].length_mm, 3.0);
        assert_eq!(found[1].volume_mm3, 8.0);
    }
}
