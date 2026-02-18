use multi_agent_gateway::routing_policy::{
    RouteScope, RouteTarget, RoutingContext, RoutingPolicyEngine, RoutingRule,
};

#[test]
fn test_precedence_channel_over_account_and_peer() {
    let engine = RoutingPolicyEngine::new(vec![
        RoutingRule::force_complex("peer-rule", RouteScope::Peer, "peer-a", "peer path", 99),
        RoutingRule::force_fast("account-rule", RouteScope::Account, "acct-a", "search", 99),
        RoutingRule::force_fast(
            "channel-rule",
            RouteScope::Channel,
            "chan-a",
            "calculator",
            1,
        ),
    ]);

    let ctx = RoutingContext {
        channel: Some("chan-a".to_string()),
        account: Some("acct-a".to_string()),
        peer: Some("peer-a".to_string()),
    };

    let decision = engine.resolve(&ctx).expect("must resolve");
    assert_eq!(decision.rule_id, "channel-rule");
    match decision.target {
        RouteTarget::FastAction { ref tool_name } => assert_eq!(tool_name, "calculator"),
        _ => panic!("expected fast action"),
    }
}

#[test]
fn test_same_scope_tie_breaks_by_priority_then_rule_id() {
    let engine = RoutingPolicyEngine::new(vec![
        RoutingRule::force_fast("b", RouteScope::Account, "acct-a", "search", 10),
        RoutingRule::force_fast("a", RouteScope::Account, "acct-a", "calculator", 10),
        RoutingRule::force_complex("c", RouteScope::Account, "acct-a", "complex", 1),
    ]);
    let ctx = RoutingContext {
        channel: None,
        account: Some("acct-a".to_string()),
        peer: None,
    };

    let decision = engine.resolve(&ctx).expect("must resolve");
    assert_eq!(decision.rule_id, "a");
}

#[test]
fn test_simulation_returns_deterministic_output() {
    let engine = RoutingPolicyEngine::new(vec![RoutingRule::force_fast(
        "channel-rule",
        RouteScope::Channel,
        "chan-a",
        "search",
        1,
    )]);
    let scenarios = vec![
        RoutingContext {
            channel: Some("chan-a".to_string()),
            account: None,
            peer: None,
        },
        RoutingContext {
            channel: Some("chan-x".to_string()),
            account: None,
            peer: None,
        },
    ];

    let sim = engine.simulate(&scenarios);
    assert_eq!(sim.len(), 2);
    assert_eq!(sim[0].matched_rule_id.as_deref(), Some("channel-rule"));
    assert_eq!(sim[1].matched_rule_id, None);
}
