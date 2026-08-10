pub mod platform;

use accesskit::{
    Action, ActionData, ActionHandler, ActionRequest, Node, NodeId, Role, TextPosition,
    TextSelection, Tree, TreeId, TreeUpdate,
};
use std::sync::mpsc::Sender;
use unicode_segmentation::UnicodeSegmentation;

const ROOT: NodeId = NodeId(0);
const TERM: NodeId = NodeId(1);
const LINE_BASE: u64 = 100; // Zeile i => NodeId(LINE_BASE + i)

pub enum AppEvent {
    RouteTo { line: usize, grapheme_col: usize },
    /// A copy-mode key pressed while the accessibility bridge window has
    /// focus (Windows only); the console doesn't receive keystrokes then,
    /// so the bridge forwards them here.
    Key(Key),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Exit,
}

/// Translates AccessKit `ActionRequest`s (routing-key presses, forwarded by
/// the screen reader as `Action::SetTextSelection`) into `AppEvent`s.
/// Runs on whatever thread the platform adapter calls it on, so it only
/// ever pushes onto a channel rather than touching app state directly.
pub struct RoutingActionHandler {
    tx: Sender<AppEvent>,
}

impl RoutingActionHandler {
    pub fn new(tx: Sender<AppEvent>) -> Self {
        Self { tx }
    }
}

impl ActionHandler for RoutingActionHandler {
    fn do_action(&mut self, request: ActionRequest) {
        if request.action != Action::SetTextSelection {
            return;
        }
        let Some(ActionData::SetTextSelection(selection)) = request.data else {
            return;
        };
        let node_id = selection.focus.node.0;
        if node_id < LINE_BASE {
            return;
        }
        let _ = self.tx.send(AppEvent::RouteTo {
            line: (node_id - LINE_BASE) as usize,
            grapheme_col: selection.focus.character_index,
        });
    }
}

pub struct A11y {
    adapter: platform::Adapter,
}

impl A11y {
    pub fn new(action_tx: Sender<AppEvent>) -> Self {
        Self {
            adapter: platform::Adapter::new(
                RoutingActionHandler::new(action_tx.clone()),
                action_tx,
            ),
        }
    }

    fn line_id(i: usize) -> NodeId {
        NodeId(LINE_BASE + i as u64)
    }

    fn build_line(text: &str) -> Node {
        let mut n = Node::new(Role::TextRun);
        n.set_value(text.to_string());
        let lengths: Vec<u8> = text.graphemes(true).map(|g| g.len() as u8).collect();
        n.set_character_lengths(lengths);
        n
    }

    fn build_update(
        &self,
        lines: &[String],
        cursor_line: usize,
        cursor_col: usize,
    ) -> TreeUpdate {
        let mut term = Node::new(Role::Terminal);
        term.set_children((0..lines.len()).map(Self::line_id).collect::<Vec<_>>());

        let pos = TextPosition {
            node: Self::line_id(cursor_line),
            character_index: cursor_col,
        };
        term.set_text_selection(TextSelection {
            anchor: pos,
            focus: pos,
        });

        let mut root = Node::new(Role::Window);
        root.set_children(vec![TERM]);

        let mut nodes = vec![(ROOT, root), (TERM, term)];
        nodes.extend(
            lines
                .iter()
                .enumerate()
                .map(|(i, l)| (Self::line_id(i), Self::build_line(l))),
        );

        TreeUpdate {
            nodes,
            tree: Some(Tree::new(ROOT)),
            tree_id: TreeId::ROOT,
            focus: TERM,
        }
    }

    pub fn on_cursor_moved(&mut self, lines: &[String], cursor_line: usize, cursor_col: usize) {
        let update = self.build_update(lines, cursor_line, cursor_col);
        self.adapter.update_if_active(|| update);
    }
}
