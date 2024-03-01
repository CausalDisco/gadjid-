// SPDX-License-Identifier: MPL-2.0
//! Implements the Parent Adjustment Intervention Distance (Parent-AID) algorithm

use rayon::prelude::*;
use rustc_hash::FxHashSet;

use crate::{
    graph_operations::{get_nam, get_nam_nvas, possible_descendants},
    PDAG,
};
 
/// Computes the parent adjustment intervention distance
/// between an estimated `guess` DAG or CPDAG and the true `truth` DAG or CPDAG
/// (a PDAG is used for internal representation, but every PDAG is assumed either a DAG or a CPDAG
///  currently distances between general PDAGs are not implemented)
/// Returns a tuple of (normalized error (in \[0,1]), total number of errors)
// This function largely overlaps with ancestor_aid in ancestor_aid.rs; differences ---highlighted--- below
pub fn parent_aid(truth: &PDAG, guess: &PDAG) -> (f64, usize) {
    assert!(
        guess.n_nodes == truth.n_nodes,
        "both graphs must contain the same number of nodes"
    );
    assert!(guess.n_nodes >= 2, "graph must contain at least 2 nodes");

    let verifier_mistakes_found = (0..guess.n_nodes)
        .into_par_iter()
        .map(|treatment| {
            let nam_in_guess = if matches!(
                guess.pdag_type,
                crate::partially_directed_acyclic_graph::Structure::CPDAG
            ) {
                get_nam(guess, &[treatment])
            } else {
                FxHashSet::<usize>::default()
            };

            // --- this function differs from ancestor_aid.rs only in the imports and from here
            let parent_adjustment = guess.parents_of(treatment).iter().copied().collect();

            // in line with the original SID, claim all NonParents may be effects
            // (this is a larger set than the NonDescendants in ancestor_aid and oset_aid;
            //  that is, the validity of the adjustment set is also checked
            //  for the additional non-effect nodes in NonParents\NonDescendants)
            let claim_possible_effect = FxHashSet::from_iter(
                (0..truth.n_nodes).filter(|v| !guess.parents_of(treatment).contains(v)),
            );

            // now we take a look at the nodes in the true graph for which the adj.set. was not valid.
            let (nam_in_true, nvas_in_true) = get_nam_nvas(truth, &[treatment], parent_adjustment);
            // --- to here
            let t_poss_desc_in_truth = possible_descendants(truth, [treatment].iter());

            let mut mistakes = 0;
            for y in 0..truth.n_nodes {
                if y == treatment {
                    continue; // this case is always correct
                }
                // if y is not claimed to be effect of t based on the guess graph
                if !claim_possible_effect.contains(&y) {
                    // but possibly a descendant of t in the truth graph.
                    if t_poss_desc_in_truth.contains(&y) {
                        // the ancestral order might be wrong, so
                        // we count a mistake
                        mistakes += 1;
                    }
                } else {
                    // if y is not amenable in guess
                    if nam_in_guess.contains(&y) {
                        // but it is amenable in truth
                        if !nam_in_true.contains(&y) {
                            // we count a mistake
                            mistakes += 1;
                        }
                    }
                    // if we reach this point, y has a VAS in guess
                    // now, if the adjustment set is not valid in truth
                    // (either because the pair (t,y) is not amenable or because the VAS is not valid)
                    else if nvas_in_true.contains(&y) {
                        // we count a mistake
                        mistakes += 1;
                    }
                }
            }

            mistakes
        })
        .sum();

    let n = guess.n_nodes;
    let comparisons = n * n - n;
    (
        verifier_mistakes_found as f64 / comparisons as f64,
        verifier_mistakes_found,
    )
}

#[cfg(test)]
mod tests {
    use std::{fs::read, io::{BufRead, Write}};

    use crate::{graph_operations::parent_aid, PDAG};

    #[test]
    fn property_equal_dags_zero_distance() {
        for n in 2..40 {
            for _rep in 0..2 {
                let dag = PDAG::random_dag(0.5, n);
                assert_eq!(
                    (0.0, 0),
                    parent_aid(&dag, &dag),
                    "parent_aid between same dags of size {n} must be zero, dag: {}",
                    dag
                );
                print!(".");
                let _ = std::io::stdout().flush();
            }
        }
    }

    #[test]
    fn random_inputs_no_crash() {
        for n in 2..40 {
            for _rep in 0..2 {
                let dag1 = PDAG::random_dag(1.0, n);
                let dag2 = PDAG::random_dag(1.0, n);
                parent_aid(&dag1, &dag2);
                let _ = std::io::stdout().flush();
            }
        }
    }

    #[test]
    fn sid_paper_test() {
        // Comparing the computed SID with the examples listed in the original SID (structural intervention distance) paper
        let g = vec![
            vec![0, 1, 1, 1, 1],
            vec![0, 0, 1, 1, 1],
            vec![0, 0, 0, 0, 0],
            vec![0, 0, 0, 0, 0],
            vec![0, 0, 0, 0, 0],
        ];
        let h1 = vec![
            vec![0, 1, 1, 1, 1],
            vec![0, 0, 1, 1, 1],
            vec![0, 0, 0, 1, 0],
            vec![0, 0, 0, 0, 0],
            vec![0, 0, 0, 0, 0],
        ];
        let h2 = vec![
            vec![0, 0, 1, 1, 1],
            vec![1, 0, 1, 1, 1],
            vec![0, 0, 0, 0, 0],
            vec![0, 0, 0, 0, 0],
            vec![0, 0, 0, 0, 0],
        ];
        let g_dag = PDAG::from_vecvec(g);
        let h1_dag = PDAG::from_vecvec(h1);
        let h2_dag = PDAG::from_vecvec(h2);

        assert_eq!(parent_aid(&g_dag, &h1_dag), (0.0, 0));
        assert_eq!(parent_aid(&g_dag, &h2_dag), (0.4, 8));
    }


    #[test]
    fn parent_aid_against_r_sid() {
        // get the root of the project
        let root = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        // get the parent directory of the project
        let root = std::path::Path::new(&root).parent().unwrap();
        // get the child dir "testgraphs"
        let root = root.join("testgraphs");


        let testcases_csv = root.join("SID.DAG-100.csv");
        let testcases = std::fs::File::open(&testcases_csv).unwrap();
        let reader = std::io::BufReader::new(testcases);
        let mut cases = reader.lines();
        cases.next(); // skip header

        let tests = cases.map(|line| {
            let line = line.unwrap();
            let mut iter = line.split(",");
            let g_true = iter.next().unwrap().parse::<usize>().unwrap();
            let g_guess = iter.next().unwrap().parse::<usize>().unwrap();
            let _ = iter.next().unwrap().parse::<usize>().unwrap();
            let r_sid = iter.next().unwrap().parse::<usize>().unwrap();
            (g_true, g_guess, r_sid)
        });


        let load_dag = |name : &str| {

            let fullname = format!("{name}.DAG-100.mtx");

            let path = root.join(fullname);

            // read the text file
            let file = std::fs::File::open(&path).unwrap();
            let reader = std::io::BufReader::new(file);

            let mut lines = reader.lines();

            // skip first two that give dimensions (always 100x100 in this case)
            lines.next();
            lines.next();
            
            let mut adj: [[i8; 100]; 100] = [[0; 100]; 100];

            for line in lines {
                let line = line.unwrap();
                let mut iter = line.split_whitespace();
                let i = iter.next().unwrap().parse::<usize>().unwrap();
                let j = iter.next().unwrap().parse::<usize>().unwrap();

                adj[i-1][j-1] = 1;
            }

            PDAG::from_vecvec(adj.iter().map(|x| x.into()).collect())
        };
        
        for (gtrue, gguess, rsid) in tests {
            let g_true = load_dag(&format!("{}", gtrue));
            let g_guess = load_dag(&format!("{}", gguess));

            let (_, mistakes) = parent_aid(&g_true, &g_guess);

            assert_eq!(mistakes, rsid);
        }

        ()
    }
}
