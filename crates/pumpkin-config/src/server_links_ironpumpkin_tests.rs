use super::*;

#[test]
fn bug_report_points_to_ironpumpkin() {
    assert_eq!(
        ServerLinksConfig::default().bug_report,
        "https://github.com/EdenNetworkItalia/IronPumpkin/issues"
    );
}
