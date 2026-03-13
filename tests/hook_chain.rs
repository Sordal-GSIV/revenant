use revenant::hook_chain::HookChain;

#[test]
fn test_no_hooks_passes_through() {
    let chain = HookChain::new();
    assert_eq!(chain.process_sync("hello\n"), Some("hello\n".to_string()));
}

#[test]
fn test_sync_hook_modifies_line() {
    let mut chain = HookChain::new();
    chain.add_sync("test", |line: &str| Some(line.replace("hello", "goodbye")));
    assert_eq!(chain.process_sync("hello\n"), Some("goodbye\n".to_string()));
}

#[test]
fn test_sync_hook_suppresses_line() {
    let mut chain = HookChain::new();
    chain.add_sync("suppress", |_| None);
    assert_eq!(chain.process_sync("gone\n"), None);
}

#[test]
fn test_hook_ordering() {
    let mut chain = HookChain::new();
    chain.add_sync("first",  |l: &str| Some(format!("[1]{l}")));
    chain.add_sync("second", |l: &str| Some(format!("[2]{l}")));
    assert_eq!(chain.process_sync("x"), Some("[2][1]x".to_string()));
}

#[test]
fn test_hook_dedup_on_same_name() {
    let mut chain = HookChain::new();
    chain.add_sync("h", |l: &str| Some(format!("[a]{l}")));
    chain.add_sync("h", |l: &str| Some(format!("[b]{l}")));  // replaces first
    assert_eq!(chain.process_sync("x"), Some("[b]x".to_string()));
}

#[test]
fn test_hook_removal() {
    let mut chain = HookChain::new();
    chain.add_sync("gone", |_| None);
    chain.remove("gone");
    assert_eq!(chain.process_sync("pass"), Some("pass".to_string()));
}
