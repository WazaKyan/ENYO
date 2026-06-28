//! Tests de la couche LLM SANS réseau : parsing/validation des décisions et
//! fallback déterministe quand il n'y a pas de client.

use llm::{direct, parse_actions};
use proto::Command;
use sim::World;

#[test]
fn parses_and_clamps_valid_actions() {
    let w = World::new(1, 50, 40);
    let json = r#"{"reasoning":"x","actions":[
        {"lever":"grievance","from":1,"to":0,"amount":5},
        {"lever":"blight","x":10,"y":10,"amount":20},
        {"lever":"windfall","x":5,"y":5,"amount":50}
    ]}"#;
    let cmds = parse_actions(json, &w).expect("json valide");
    assert_eq!(cmds.len(), 3);
    assert!(matches!(
        cmds[0],
        Command::DirectorGrievance {
            from: 1,
            to: 0,
            amount: 5
        }
    ));
    assert!(matches!(
        cmds[1],
        Command::DirectorBlight {
            x: 10,
            y: 10,
            amount: 20
        }
    ));
    // 50 dépasse le plafond windfall (40) -> borné.
    assert!(matches!(
        cmds[2],
        Command::DirectorWindfall { amount: 40, .. }
    ));
}

#[test]
fn drops_out_of_bounds_and_rejects_bad_json() {
    let w = World::new(1, 50, 40);
    let json = r#"{"actions":[{"lever":"blight","x":999,"y":999,"amount":10}]}"#;
    assert!(
        parse_actions(json, &w).expect("json valide").is_empty(),
        "cible hors carte -> ignorée"
    );
    assert!(parse_actions("ceci n'est pas du json", &w).is_none());
}

#[test]
fn extracts_json_from_markdown_fences() {
    let w = World::new(1, 50, 40);
    let wrapped =
        "```json\n{\"actions\":[{\"lever\":\"grievance\",\"from\":2,\"to\":0,\"amount\":3}]}\n```";
    let cmds = parse_actions(wrapped, &w).expect("extraction du bloc JSON");
    assert_eq!(cmds.len(), 1);
}

#[test]
fn no_client_means_deterministic_no_network() {
    let w = World::new(2, 60, 40); // aucune nation
    let cmds = direct(&w, 0, None);
    assert!(
        cmds.is_empty(),
        "sans client ni nation : aucune action, aucun réseau"
    );
}
