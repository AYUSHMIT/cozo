/*
 * Copyright 2022, The Cozo Project Authors.
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
 * If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use std::collections::BTreeMap;

use itertools::Itertools;
use miette::Result;
use rand::prelude::*;
use smartstring::{LazyCompact, SmartString};

use crate::algo::{AlgoImpl, AlgoPayload};
use crate::data::expr::Expr;
use crate::data::symb::Symbol;
use crate::data::value::DataValue;
use crate::parse::SourceSpan;
use crate::runtime::db::Poison;
use crate::runtime::temp_store::RegularTempStore;

pub(crate) struct LabelPropagation;

impl AlgoImpl for LabelPropagation {
    fn run(
        &self,
        payload: AlgoPayload<'_, '_>,
        out: &mut RegularTempStore,
        poison: Poison,
    ) -> Result<()> {
        let edges = payload.get_input(0)?;
        let undirected = payload.bool_option("undirected", Some(false))?;
        let max_iter = payload.pos_integer_option("max_iter", Some(10))?;
        let (graph, indices, _inv_indices, _) =
            edges.convert_edge_to_weighted_graph(undirected, true)?;
        let labels = label_propagation(&graph, max_iter, poison)?;
        for (idx, label) in labels.into_iter().enumerate() {
            let node = indices[idx].clone();
            out.put(vec![DataValue::from(label as i64), node]);
        }
        Ok(())
    }

    fn arity(
        &self,
        _options: &BTreeMap<SmartString<LazyCompact>, Expr>,
        _rule_head: &[Symbol],
        _span: SourceSpan,
    ) -> Result<usize> {
        Ok(2)
    }
}

fn label_propagation(
    graph: &[Vec<(usize, f64)>],
    max_iter: usize,
    poison: Poison,
) -> Result<Vec<usize>> {
    let n_nodes = graph.len();
    let mut labels = (0..n_nodes).collect_vec();
    let mut rng = thread_rng();
    let mut iter_order = (0..n_nodes).collect_vec();
    for _ in 0..max_iter {
        iter_order.shuffle(&mut rng);
        let mut changed = false;
        for node in &iter_order {
            let mut labels_for_node: BTreeMap<usize, f64> = BTreeMap::new();
            let neighbours = &graph[*node];
            if neighbours.is_empty() {
                continue;
            }
            for (to_node, weight) in neighbours {
                let label = labels[*to_node];
                *labels_for_node.entry(label).or_default() += *weight;
            }
            let mut labels_by_score = labels_for_node.into_iter().collect_vec();
            labels_by_score.sort_by(|a, b| a.1.total_cmp(&b.1).reverse());
            let max_score = labels_by_score[0].1;
            let candidate_labels = labels_by_score
                .into_iter()
                .take_while(|(_, score)| *score == max_score)
                .map(|(l, _)| l)
                .collect_vec();
            let new_label = *candidate_labels.choose(&mut rng).unwrap();
            if new_label != labels[*node] {
                changed = true;
                labels[*node] = new_label;
            }
            poison.check()?;
        }
        if !changed {
            break;
        }
    }
    Ok(labels)
}
