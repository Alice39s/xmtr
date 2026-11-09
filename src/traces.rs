//! Uploaded traces: the admin link and the delete command printed after an upload.

use crate::model::UploadAccepted;

/// Labels in front of trace details are padded to the width of `trace-id`, so the values line up.
const LABEL_WIDTH: usize = 8;

/// Printed under the admin link and the delete command.
pub const ADMIN_WARNING: &str = "Anyone with the admin link can delete this trace.";

/// `name` padded to the label width, with the space before the value.
pub fn label(name: &str) -> String {
    format!("{name:<LABEL_WIDTH$} ")
}

/// The admin link and the command that deletes the trace, each behind its label, for the fields
/// the gateway returned.
pub fn admin_lines(trace: &UploadAccepted) -> Vec<(String, String)> {
    let link = trace.admin_url.clone().map(|url| (label("admin"), url));
    let delete = trace
        .delete_command()
        .map(|command| (label("delete"), command));
    link.into_iter().chain(delete).collect()
}

/// The admin lines and the warning under them as plain text; none when the gateway returned no
/// admin link.
pub fn admin_text(trace: &UploadAccepted) -> Vec<String> {
    let mut lines: Vec<String> = admin_lines(trace)
        .into_iter()
        .map(|(label, value)| label + &value)
        .collect();
    if !lines.is_empty() {
        lines.push(ADMIN_WARNING.to_owned());
    }
    lines
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub const ID: &str = "103115083070058496-nBjPEnny";
    pub const KEY: &str = "1b4e28ba-2fa1-4d2b-883f-0016d3cca427";

    /// An upload accepted by a gateway that returns the admin key and its links.
    pub fn accepted() -> UploadAccepted {
        UploadAccepted {
            id: ID.to_owned(),
            pin: false,
            url: format!("https://xmsl.dev/xmtr/{ID}"),
            admin_key: Some(KEY.to_owned()),
            admin_url: Some(format!(
                "https://xmsl.dev/ipmap?plat=xmtr&trace-id={ID}&admin-key={KEY}"
            )),
            admin_api_url: Some(format!(
                "https://stage-info-api.xmsl.dev/api/v1/traces/{ID}?admin-key={KEY}"
            )),
        }
    }

    #[test]
    fn lines_up_the_admin_link_and_delete_command_with_the_trace_id() {
        let text = admin_text(&accepted());
        assert_eq!(
            text,
            [
                format!("admin    https://xmsl.dev/ipmap?plat=xmtr&trace-id={ID}&admin-key={KEY}"),
                format!(
                    "delete   curl -X DELETE 'https://stage-info-api.xmsl.dev/api/v1/traces/{ID}?admin-key={KEY}'"
                ),
                ADMIN_WARNING.to_owned(),
            ]
        );
        assert_eq!(label("trace-id"), "trace-id ");
    }

    #[test]
    fn prints_nothing_for_gateways_without_admin_links() {
        let trace = UploadAccepted {
            admin_key: None,
            admin_url: None,
            admin_api_url: None,
            ..accepted()
        };
        assert!(admin_text(&trace).is_empty());
    }
}
