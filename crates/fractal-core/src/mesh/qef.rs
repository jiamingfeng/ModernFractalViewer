//! Quadratic Error Function (QEF) solver for Dual Contouring.
//!
//! Minimises `‖A·x − b‖²` where each row of A is a surface normal and each
//! element of b is `dot(normal, intersection_point)`.  The solution is the
//! point that best fits all the half-planes defined by edge crossings.
//!
//! Uses the normal equations `(AᵀA)x = Aᵀb` with a direct 3×3 symmetric
//! solver.  When the system is rank-deficient the solution is biased toward
//! the *mass point* (average of intersection points) along the degenerate
//! directions via a regularisation term.

/// Accumulates half-plane constraints and solves for an optimal vertex.
#[derive(Clone, Debug)]
pub struct QefSolver {
    /// Upper triangle of AᵀA (symmetric 3×3):
    /// `[a00, a01, a02, a11, a12, a22]`
    ata: [f64; 6],
    /// AᵀB vector
    atb: [f64; 3],
    /// Sum of intersection points (for mass-point fallback)
    point_sum: [f64; 3],
    /// Number of half-planes added
    count: u32,
}

impl QefSolver {
    /// Creates an empty solver with no constraints.
    pub fn new() -> Self {
        Self {
            ata: [0.0; 6],
            atb: [0.0; 3],
            point_sum: [0.0; 3],
            count: 0,
        }
    }

    /// Returns the number of planes added so far.
    #[inline]
    #[allow(dead_code)]
    pub fn count(&self) -> u32 {
        self.count
    }

    /// Returns the mass point (average of all intersection points added).
    pub fn mass_point(&self) -> [f32; 3] {
        if self.count == 0 {
            return [0.0; 3];
        }
        let inv = 1.0 / self.count as f64;
        [
            (self.point_sum[0] * inv) as f32,
            (self.point_sum[1] * inv) as f32,
            (self.point_sum[2] * inv) as f32,
        ]
    }

    /// Adds a half-plane constraint: the surface passes through `point` with
    /// outward normal `normal`.
    pub fn add(&mut self, normal: [f32; 3], point: [f32; 3]) {
        let nx = normal[0] as f64;
        let ny = normal[1] as f64;
        let nz = normal[2] as f64;
        let px = point[0] as f64;
        let py = point[1] as f64;
        let pz = point[2] as f64;

        // AᵀA += n ⊗ n  (outer product)
        self.ata[0] += nx * nx; // [0,0]
        self.ata[1] += nx * ny; // [0,1]
        self.ata[2] += nx * nz; // [0,2]
        self.ata[3] += ny * ny; // [1,1]
        self.ata[4] += ny * nz; // [1,2]
        self.ata[5] += nz * nz; // [2,2]

        // Aᵀb += n * dot(n, p)
        let d = nx * px + ny * py + nz * pz;
        self.atb[0] += nx * d;
        self.atb[1] += ny * d;
        self.atb[2] += nz * d;

        self.point_sum[0] += px;
        self.point_sum[1] += py;
        self.point_sum[2] += pz;
        self.count += 1;
    }

    /// Solves the QEF and returns the optimal vertex position.
    ///
    /// The result is clamped to `[cell_min, cell_max]`.  If the system is
    /// degenerate (fewer than 3 independent planes), the solution is biased
    /// toward the mass point along the null-space directions.
    pub fn solve(&self, cell_min: [f32; 3], cell_max: [f32; 3]) -> [f32; 3] {
        if self.count == 0 {
            // No data — return cell centre
            return [
                (cell_min[0] + cell_max[0]) * 0.5,
                (cell_min[1] + cell_max[1]) * 0.5,
                (cell_min[2] + cell_max[2]) * 0.5,
            ];
        }

        let mp = self.mass_point();

        // Regularisation: add a small bias toward the mass point.
        // This makes the system always full-rank and produces the mass
        // point when there are no constraints (or all normals are parallel).
        let lambda = 0.01;
        let a = [
            self.ata[0] + lambda,
            self.ata[1],
            self.ata[2],
            self.ata[3] + lambda,
            self.ata[4],
            self.ata[5] + lambda,
        ];
        let b = [
            self.atb[0] + lambda * mp[0] as f64,
            self.atb[1] + lambda * mp[1] as f64,
            self.atb[2] + lambda * mp[2] as f64,
        ];

        // Solve 3×3 symmetric positive-definite system via Cholesky
        if let Some(x) = solve_symmetric_3x3(&a, &b) {
            // Clamp to cell bounds
            let result = [
                (x[0] as f32).clamp(cell_min[0], cell_max[0]),
                (x[1] as f32).clamp(cell_min[1], cell_max[1]),
                (x[2] as f32).clamp(cell_min[2], cell_max[2]),
            ];
            if result[0].is_finite() && result[1].is_finite() && result[2].is_finite() {
                return result;
            }
        }

        // Fallback: mass point clamped to cell
        [
            mp[0].clamp(cell_min[0], cell_max[0]),
            mp[1].clamp(cell_min[1], cell_max[1]),
            mp[2].clamp(cell_min[2], cell_max[2]),
        ]
    }
}

/// Solves a 3×3 symmetric positive-definite system `M·x = rhs` using
/// Cholesky decomposition.  Returns `None` if the matrix is not SPD.
///
/// `m` is stored as upper triangle: `[m00, m01, m02, m11, m12, m22]`
fn solve_symmetric_3x3(m: &[f64; 6], rhs: &[f64; 3]) -> Option<[f64; 3]> {
    // Cholesky: M = LLᵀ where L is lower-triangular
    //
    //  L = [ l00   0    0  ]
    //      [ l10  l11   0  ]
    //      [ l20  l21  l22 ]

    let l00 = m[0].sqrt();
    if l00 < 1e-15 {
        return None;
    }
    let l10 = m[1] / l00;
    let l20 = m[2] / l00;

    let l11_sq = m[3] - l10 * l10;
    if l11_sq < 1e-15 {
        return None;
    }
    let l11 = l11_sq.sqrt();
    let l21 = (m[4] - l20 * l10) / l11;

    let l22_sq = m[5] - l20 * l20 - l21 * l21;
    if l22_sq < 1e-15 {
        return None;
    }
    let l22 = l22_sq.sqrt();

    // Forward substitution: L·y = rhs
    let y0 = rhs[0] / l00;
    let y1 = (rhs[1] - l10 * y0) / l11;
    let y2 = (rhs[2] - l20 * y0 - l21 * y1) / l22;

    // Back substitution: Lᵀ·x = y
    let x2 = y2 / l22;
    let x1 = (y1 - l21 * x2) / l11;
    let x0 = (y0 - l10 * x1 - l20 * x2) / l00;

    Some([x0, x1, x2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_plane_gives_mass_point() {
        let mut qef = QefSolver::new();
        qef.add([0.0, 1.0, 0.0], [0.5, 0.3, 0.5]);

        let result = qef.solve([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        // With one plane, the QEF is rank-1; regularisation pulls toward
        // mass point for the unconstrained axes.
        assert!((result[1] - 0.3).abs() < 0.05, "y should be near 0.3, got {}", result[1]);
    }

    #[test]
    fn three_orthogonal_planes() {
        let mut qef = QefSolver::new();
        // Three planes through (0.25, 0.5, 0.75)
        qef.add([1.0, 0.0, 0.0], [0.25, 0.5, 0.75]);
        qef.add([0.0, 1.0, 0.0], [0.25, 0.5, 0.75]);
        qef.add([0.0, 0.0, 1.0], [0.25, 0.5, 0.75]);

        let result = qef.solve([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        assert!((result[0] - 0.25).abs() < 0.02, "x={}", result[0]);
        assert!((result[1] - 0.50).abs() < 0.02, "y={}", result[1]);
        assert!((result[2] - 0.75).abs() < 0.02, "z={}", result[2]);
    }

    #[test]
    fn result_clamped_to_cell() {
        let mut qef = QefSolver::new();
        // All planes push the solution way outside the cell
        qef.add([1.0, 0.0, 0.0], [10.0, 0.0, 0.0]);
        qef.add([0.0, 1.0, 0.0], [0.0, 10.0, 0.0]);
        qef.add([0.0, 0.0, 1.0], [0.0, 0.0, 10.0]);

        let result = qef.solve([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        assert!(result[0] <= 1.0 && result[0] >= 0.0);
        assert!(result[1] <= 1.0 && result[1] >= 0.0);
        assert!(result[2] <= 1.0 && result[2] >= 0.0);
    }

    #[test]
    fn empty_solver_returns_cell_centre() {
        let qef = QefSolver::new();
        let result = qef.solve([0.0, 0.0, 0.0], [2.0, 4.0, 6.0]);
        assert!((result[0] - 1.0).abs() < 1e-6);
        assert!((result[1] - 2.0).abs() < 1e-6);
        assert!((result[2] - 3.0).abs() < 1e-6);
    }

    #[test]
    fn corner_from_two_perpendicular_planes() {
        let mut qef = QefSolver::new();
        // Two planes forming an edge at x=0.3, z=0.7
        qef.add([1.0, 0.0, 0.0], [0.3, 0.5, 0.5]);
        qef.add([0.0, 0.0, 1.0], [0.5, 0.5, 0.7]);

        let result = qef.solve([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        assert!((result[0] - 0.3).abs() < 0.05, "x={}", result[0]);
        assert!((result[2] - 0.7).abs() < 0.05, "z={}", result[2]);
    }

    #[test]
    fn solve_symmetric_identity() {
        // M = identity, rhs = [1, 2, 3]
        let m = [1.0, 0.0, 0.0, 1.0, 0.0, 1.0];
        let rhs = [1.0, 2.0, 3.0];
        let x = solve_symmetric_3x3(&m, &rhs).unwrap();
        assert!((x[0] - 1.0).abs() < 1e-10);
        assert!((x[1] - 2.0).abs() < 1e-10);
        assert!((x[2] - 3.0).abs() < 1e-10);
    }

    #[test]
    fn solve_symmetric_general() {
        // M = [[4, 2, 1], [2, 5, 3], [1, 3, 6]], rhs = [1, 2, 3]
        let m = [4.0, 2.0, 1.0, 5.0, 3.0, 6.0];
        let rhs = [1.0, 2.0, 3.0];
        let x = solve_symmetric_3x3(&m, &rhs).unwrap();
        // Verify: M*x ≈ rhs
        let r0 = 4.0 * x[0] + 2.0 * x[1] + 1.0 * x[2];
        let r1 = 2.0 * x[0] + 5.0 * x[1] + 3.0 * x[2];
        let r2 = 1.0 * x[0] + 3.0 * x[1] + 6.0 * x[2];
        assert!((r0 - 1.0).abs() < 1e-10, "r0={r0}");
        assert!((r1 - 2.0).abs() < 1e-10, "r1={r1}");
        assert!((r2 - 3.0).abs() < 1e-10, "r2={r2}");
    }
}
