mod accesskit_windows;
use accesskit::{
    Node, NodeId, Role, TextPosition, TextSelection, Tree, TreeUpdate,
};
use unicode_segmentation::UnicodeSegmentation;

const ROOT: NodeId = NodeId(0);
const TERM: NodeId = NodeId(1);
const LINE_BASE: u64 = 100; // Zeile i => NodeId(LINE_BASE + i)

struct A11y {
    adapter: accesskit_windows::Adapter,
}

impl A11y {
    fn line_id(i: usize) -> NodeId {
        NodeId(LINE_BASE + i as u64)
    }

    fn build_line(text: &str) -> Node {
        let mut n = Node::new(Role::TextRun);
        n.set_value(text.to_string());
        // Graphem-Cluster statt chars(): ein Eintrag pro
        // sichtbarem Zeichen, korrekt für Umlaute/Emojis
        let lengths: Vec<u8> = text
            .graphemes(true)
            .map(|g| g.len() as u8)
            .collect();
        n.set_character_lengths(lengths);
        n
    }

    fn build_update(
        &self,
        lines: &[String],
        cursor_line: usize,
        cursor_col: usize, // in Graphemen, nicht Bytes!
    ) -> TreeUpdate {
        let mut term = Node::new(Role::Terminal);
        term.set_children(
            (0..lines.len()).map(Self::line_id).collect::<Vec<_>>(),
        );

        // Caret = kollabierte Selektion auf der aktiven Zeile.
        // DAS ist der Auslöser für Sprach- UND Braille-Update.
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
            focus: TERM, // Fokus auf dem Terminal-Node:
            // SR verfolgt dessen Caret
        }
    }

    fn on_cursor_moved(
        &mut self,
        lines: &[String],
        cursor_line: usize,
        cursor_col: usize,
    ) {
        let update = self.build_update(lines, cursor_line, cursor_col);
        self.adapter.update_if_active(|| update);
    }
}
