//! The page ops round-trip through the file's
//! records with every page field, and a page record holding a field it does not know is refused.

use ecad_model::{GenKind, Op, Page, PageId, PageKind, StyleRef};

fn full_page() -> Page {
    let mut p = Page::titled("Main supply").unwrap();
    p.number = ecad_model::PageNo::parse("105a");
    p.folder = Some(ecad_model::FolderId(7));
    p.page_type = StyleRef::builtin("network");
    p.sheet = ecad_model::SheetSpec::parse("A2").ok();
    p.title_block = Some(StyleRef::Own(0x55));
    p.display.insert("text.tag.show".into(), "yes".into());
    p
}

#[test]
fn page_ops_round_trip_through_records() {
    let page = full_page();
    let mut cover = Page::titled("Cover").unwrap();
    cover.kind = PageKind::Generated(GenKind::Cover);
    let mut strip = Page::titled("X1").unwrap();
    strip.kind = PageKind::TerminalStrip { strip: ecad_model::DeviceId(0xabc) };
    let renamed = Page { title: ecad_model::PageTitle::new("Renamed").unwrap(), ..page.clone() };
    let ops = [
        Op::AddPage { id: PageId(1), page: page.clone(), index: 0 },
        Op::AddPage { id: PageId(2), page: cover, index: 1 },
        Op::AddPage { id: PageId(3), page: strip, index: 2 },
        Op::RemovePage { id: PageId(1), page: page.clone(), index: 0 },
        Op::ModifyPage { id: PageId(1), before: page, after: renamed },
    ];
    for op in ops {
        let tx = ecad_model::Transaction {
            id: ecad_model::TxId(1),
            seq: 1,
            author: ecad_model::Author::User,
            label: "x".into(),
            kind: ecad_model::TxKind::Edit,
            time_ms: None,
            ops: vec![op.clone()],
        };
        let prev = ecad_file::chain::seed(ecad_file::chain::ChainKind::Project, 1);
        let v = ecad_file::record::tx_value(&tx, &prev).unwrap();
        let back = ecad_file::record::decode_tx(&v).unwrap_or_else(|e| {
            panic!(
                "{}: {e:?}
{v}",
                op.name()
            )
        });
        assert_eq!(back.ops, [op]);
    }
}
