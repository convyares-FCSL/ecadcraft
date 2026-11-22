//! Rules of `terminal_strips::accessories` (moved from its unit tests): a document or text in, a result out.

use ecad_electrical::terminal_strips::accessories::*;

#[test]
fn places_and_values() {
    assert_eq!(spot_of("acc.after.4"), Some(Spot::After("4".into())));
    assert_eq!(spot_of("acc.on.PE1"), Some(Spot::On("PE1".into())));
    assert_eq!(spot_of("acc.middle"), None);
    assert_eq!(acc_value(&Spot::End, Some(" End plate , end-stop ")), Ok(Some("end_plate,end_stop".into())));
    assert_eq!(acc_value(&Spot::Start, Some(" ")), Ok(None));
    assert!(acc_value(&Spot::Start, Some("plate")).unwrap_err().to_string().contains("Write end_stop, end_plate, partition or strip_label"));
    assert!(acc_value(&Spot::On("4".into()), Some("partition")).unwrap_err().to_string().contains("Only marker goes on a position"));
    assert!(acc_value(&Spot::Start, Some("marker")).is_err());
    assert!(acc_value(&Spot::Start, Some(&"end_stop,".repeat(9))).is_err());
    assert!(acc_value(&Spot::Start, Some(&"end_stop,".repeat(1000))).is_err());
    assert_eq!(project_key("end_stop.part_number"), "strip_end_stop_part_number");
    assert_eq!(strip_key_of_project("strip_jumper_bar").as_deref(), Some("jumper.bar"));
    assert_eq!(strip_key_of_project("strip_marker_per_card").as_deref(), Some("marker.per_card"));
    assert!(part_keys().iter().all(|k| project_key(k).len() <= 40));
    assert_eq!(jumper_bar_part_key("jumper_bar.2.part_number"), Some((2, "part_number")));
    assert_eq!(jumper_bar_part_key("jumper_bar.02.part_number"), None);
    assert_eq!(jumper_bar_part_key("jumper_bar.2.colour"), None);
    assert!(check_label(&"x".repeat(17)).is_err());
}
