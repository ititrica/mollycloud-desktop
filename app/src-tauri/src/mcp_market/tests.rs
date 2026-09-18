use super::*;

#[test]
fn four_agent_formats_preserve_settings_and_comments() {
    let source = json!({"type":"stdio","command":"C:/node.exe","args":["C:/MCP/server.js"],"env":{"TEST_TOKEN":"mock-token"}});
    for agent in ["codex", "claude", "opencode", "gemini"] {
        let original = if agent == "codex" {
            "# user's model\nmodel = \"existing\"\n[mcp_servers.other]\ncommand = \"keep\"\n"
        } else if agent == "opencode" {
            "{\n// user's model\n\"model\":\"existing\",\"mcp\":{\"other\":{\"type\":\"local\",\"command\":[\"keep\"]}},\n}"
        } else {
            "{\n// user's model\n\"model\":\"existing\",\"mcpServers\":{\"other\":{\"command\":\"keep\"}},\n}"
        };
        let target = configs::target_spec(agent, &source).unwrap();
        let patched = configs::patch(agent, original, "new-server", Some(&target)).unwrap();
        assert!(patched.contains("user's model"));
        assert!(patched.contains("existing"));
        let entries = configs::entries(agent, &patched).unwrap();
        assert_eq!(entries["new-server"], target);
        assert!(entries.contains_key("other"));
        let removed = configs::patch(agent, &patched, "new-server", None).unwrap();
        assert_eq!(
            configs::entries(agent, &removed).unwrap(),
            configs::entries(agent, original).unwrap()
        );
    }
}
#[test]
fn remote_headers_map_to_each_agent() {
    let spec = json!({"type":"http","url":"https://example.test/mcp","headers":{"Authorization":"Bearer mock-token"}});
    assert_eq!(
        configs::target_spec("codex", &spec).unwrap()["http_headers"],
        spec["headers"]
    );
    assert_eq!(
        configs::target_spec("gemini", &spec).unwrap()["httpUrl"],
        spec["url"]
    );
    assert_eq!(
        configs::target_spec("opencode", &spec).unwrap()["type"],
        "remote"
    );
    assert!(
        configs::target_spec("codex", &json!({"type":"sse","url":"https://example.test"})).is_err()
    );
}
#[test]
fn malformed_and_nonobject_configs_are_never_rebuilt() {
    for (agent, source) in [
        ("codex", "mcp_servers = 1"),
        ("codex", "[broken"),
        ("claude", "[]"),
        ("opencode", "{mcp: false}"),
        ("gemini", "{mcpServers: []}"),
    ] {
        assert!(configs::patch(agent, source, "new", Some(&json!({"command":"test"}))).is_err());
    }
}
#[test]
fn inline_toml_server_table_survives() {
    let text = "model = 'keep'\nmcp_servers = { old = { command = 'keep' } }\n";
    let patched = configs::patch("codex", text, "new", Some(&json!({"command":"test"}))).unwrap();
    assert_eq!(configs::entries("codex", &patched).unwrap().len(), 2);
}
#[test]
fn collisions_and_external_edits_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("settings.json");
    std::fs::write(&path, r#"{"mcpServers":{"existing":{"command":"keep"}}}"#).unwrap();
    assert!(prepare_change(
        "claude",
        &path,
        "existing",
        None,
        Some(&json!({"command":"new"}))
    )
    .is_err());
    let change = prepare_change(
        "claude",
        &path,
        "new",
        None,
        Some(&json!({"command":"new"})),
    )
    .unwrap();
    std::fs::write(&path, r#"{"changed":true}"#).unwrap();
    assert!(commit(&[change], &atomic_private, || Ok(())).is_err());
    assert_eq!(read_text(&path).unwrap(), r#"{"changed":true}"#);
}
#[test]
fn later_agent_failure_rolls_back_earlier_agent() {
    let root = tempfile::tempdir().unwrap();
    let first = root.path().join("first.json");
    let second = root.path().join("second.json");
    std::fs::write(&first, "{\"keep\":true}").unwrap();
    let changes = vec![
        prepare_change(
            "claude",
            &first,
            "test",
            None,
            Some(&json!({"command":"test"})),
        )
        .unwrap(),
        prepare_change(
            "gemini",
            &second,
            "test",
            None,
            Some(&json!({"command":"test"})),
        )
        .unwrap(),
    ];
    let writer = |path: &Path, data: &[u8]| {
        if path == second {
            Err("simulated failure".into())
        } else {
            atomic_private(path, data)
        }
    };
    assert!(commit(&changes, &writer, || Ok(())).is_err());
    assert_eq!(read_text(&first).unwrap(), "{\"keep\":true}");
    assert!(!second.exists());
}
#[test]
fn ledger_failure_restores_all_configs_including_new_files() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("new.json");
    let change = prepare_change(
        "claude",
        &path,
        "test",
        None,
        Some(&json!({"command":"test"})),
    )
    .unwrap();
    assert!(commit(&[change], &atomic_private, || Err("ledger failure".into())).is_err());
    assert!(!path.exists());
}
#[test]
fn rollback_preserves_changes_from_another_writer() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("settings.json");
    let change = prepare_change(
        "claude",
        &path,
        "test",
        None,
        Some(&json!({"command":"test"})),
    )
    .unwrap();
    let result = commit(&[change], &atomic_private, || {
        std::fs::write(&path, "{\"external\":true}").unwrap();
        Err("failure".into())
    });
    assert!(result.unwrap_err().contains("保留现场"));
    assert_eq!(read_text(&path).unwrap(), "{\"external\":true}");
}
#[test]
fn inputs_reject_path_traversal_and_invalid_connections() {
    for id in ["../test", "a/b", "", "test\n", "C:\\path"] {
        assert!(validate_id(id).is_err());
    }
    for spec in [
        json!({"type":"http","url":"file:///C:/private"}),
        json!({"command":"node","args":"shell text"}),
        json!({"type":"http","url":"https://user:secret@example.com"}),
        json!({"command":"node","env":{"x":12}}),
    ] {
        assert!(validate_spec(&spec).is_err());
    }
    assert!(validate_spec(&json!({"type":"http","url":"http://127.0.0.1:32100/mcp"})).is_ok());
}
#[test]
fn encrypted_records_never_store_plaintext_secrets() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("record.bin");
    let value = json!({"token":"test-only-secret"});
    save_encrypted(&path, &value).unwrap();
    let bytes = std::fs::read(path).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("test-only-secret"));
    assert_eq!(
        serde_json::from_slice::<Value>(&crate::dpapi_unprotect(&bytes).unwrap()).unwrap(),
        value
    );
}
#[test]
fn catalog_is_pinned_and_has_no_molly_runtime_dependency() {
    let entries = catalog::templates();
    assert_eq!(entries.len(), 21);
    let mut ids = BTreeSet::new();
    for entry in entries {
        validate_id(&entry.id).unwrap();
        assert!(ids.insert(entry.id));
        let recipe = entry.recipe.unwrap();
        if recipe.kind != "remote" {
            assert!(!recipe.version.is_empty());
            assert_ne!(recipe.version, "latest");
            assert!(!recipe.bin.contains(".."));
        } else {
            catalog::validate_url(&recipe.url).unwrap();
        }
    }
}

#[test]
fn secret_placeholders_are_not_reinterpreted() {
    let values = BTreeMap::from([
        ("token".into(), "abc{directory}{dataDir}".into()),
        ("directory".into(), "private".into()),
    ]);
    assert_eq!(
        process::expand_template("Bearer {token}", &values, "data"),
        "Bearer abc{directory}{dataDir}"
    );
}

#[test]
fn crash_recovery_distinguishes_committed_and_incomplete_transactions() {
    for committed in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("agent.json");
        std::fs::write(&path, "{\"keep\":true}").unwrap();
        let change = prepare_change(
            "claude",
            &path,
            "test",
            None,
            Some(&json!({"command":"mock"})),
        )
        .unwrap();
        std::fs::write(&path, &change.after).unwrap();
        save_encrypted(
            &root.path().join("pending.bin"),
            &(Ledger::default(), vec![change], 1_u64),
        )
        .unwrap();
        if committed {
            save_encrypted(
                &root.path().join("state.bin"),
                &Ledger {
                    revision: 1,
                    ..Ledger::default()
                },
            )
            .unwrap();
        }
        recover_with(root.path(), &atomic_private).unwrap();
        let entries = configs::entries("claude", &read_text(&path).unwrap()).unwrap();
        assert_eq!(entries.contains_key("test"), committed);
        assert!(!root.path().join("pending.bin").exists());
    }
}

#[test]
#[ignore = "Downloads real pinned public packages into a temporary directory"]
fn actual_npm_and_python_initialize() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("packages");
    for id in ["sequential-thinking", "memory", "filesystem", "time"] {
        let entry = catalog::template(id).unwrap();
        let values = BTreeMap::from([(
            "directory".into(),
            temporary.path().to_string_lossy().into(),
        )]);
        let spec =
            process::build_spec(&entry, &values, &root).unwrap_or_else(|e| panic!("{id}: {e}"));
        assert!(process::probe_stdio(&spec)
            .unwrap_or_else(|e| panic!("{id}: {e}"))
            .contains("通过"));
        let reuse = process::build_spec(&entry, &values, &root).unwrap();
        assert_eq!(spec, reuse);
        for agent in ["codex", "claude", "opencode", "gemini"] {
            let target = configs::target_spec(agent, &spec).unwrap();
            assert!(!target.to_string().contains("mollycloud"));
        }
        println!("{id}: installed pinned package, initialized and reused independently");
    }
}

#[tokio::test]
async fn http_initialize_handles_notifications_auth_errors_and_session_cleanup() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    for (code,body,session,success) in [
        (200,"data: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/message\"}\n\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"protocolVersion\":\"2025-03-26\"}}\n\n",true,true),
        (401,"unauthorized",false,true),
        (200,"{\"jsonrpc\":\"2.0\",\"id\":1,\"error\":{\"code\":-32600}}",true,false),
        (200,"invalid JSON",true,false),
    ] {
        let server=TcpListener::bind("127.0.0.1:0").unwrap();
        server.set_nonblocking(true).unwrap();
        let url=format!("http://{}/mcp",server.local_addr().unwrap());
        let thread=std::thread::spawn(move||{
            let deadline=std::time::Instant::now()+std::time::Duration::from_secs(10);
            for index in 0..if session{2}else{1} {
                let mut stream=loop {match server.accept(){Ok((s,_))=>break s,Err(_)if std::time::Instant::now()<deadline=>std::thread::sleep(std::time::Duration::from_millis(10)),Err(e)=>panic!("request missing: {e}")}};
                stream.set_nonblocking(false).unwrap();
                stream.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
                let mut bytes=Vec::new();
                let mut buffer=[0;4096];
                loop {
                    let size=stream.read(&mut buffer).unwrap();assert!(size>0);
                    bytes.extend_from_slice(&buffer[..size]);assert!(bytes.len()<16_384);
                    let text=String::from_utf8_lossy(&bytes);
                    if let Some(end)=text.find("\r\n\r\n") {
                        let len=text[..end].lines().find_map(|line|line.to_lowercase().strip_prefix("content-length:").and_then(|v|v.trim().parse::<usize>().ok())).unwrap_or(0);
                        if bytes.len()>=end+4+len {break;}
                    }
                }
                let request=String::from_utf8_lossy(&bytes);
                assert!(request.to_lowercase().contains("authorization: bearer test-only"));
                if index==0 {
                    assert!(request.starts_with("POST /mcp"));
                    let header=if session{"mcp-session-id: test-session\r\n"}else{""};
                    write!(stream,"HTTP/1.1 {code} Test\r\n{header}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
                } else {
                    assert!(request.starts_with("DELETE /mcp"));
                    assert!(request.contains("test-session"));
                    write!(stream,"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n").unwrap();
                }
            }
        });
        let spec=json!({"type":"http","url":url,"headers":{"Authorization":"Bearer test-only"}});
        let result=process::probe_http(&spec).await;
        assert_eq!(result.is_ok(),success,"{result:?}");
        if code==401{assert!(result.unwrap().contains("等待授权"));}
        thread.join().unwrap();
    }
}

#[test]
fn source_registry_variables_defaults_choices_and_optional_arguments() {
    let server = json!({"remotes":[{"type":"streamable-http","url":"https://example.test/{tenant}/mcp",
        "variables":{"tenant":{"isRequired":true,"default":"demo"}},
        "headers":[{"name":"Authorization","value":"Bearer {token}","variables":{"token":{"isRequired":true,"isSecret":true}}}]}],
        "packages":[{"registryType":"npm","identifier":"@example/server","version":"1.2.3","transport":{"type":"stdio"},
          "environmentVariables":[{"name":"MODE","default":"read","choices":["read","write"]}],
          "packageArguments":[{"type":"named","name":"--limit"},{"type":"named","name":"--verbose","value":""}]}]});
    let (recipes, notes) = sources::registry_recipes(&server);
    assert!(notes.is_empty());
    assert_eq!(recipes.len(), 2);
    let (recipe, fields) = &recipes[0];
    assert_eq!(recipe.url, "https://example.test/{input1}/mcp");
    assert_eq!(fields[0].default, "demo");
    assert_eq!(fields[1].kind, "secret");
    let mut entry = catalog::template("memory").unwrap();
    entry.recipe = Some(recipe.clone());
    entry.fields = fields.clone();
    let temp = tempfile::tempdir().unwrap();
    let spec = process::build_spec(
        &entry,
        &BTreeMap::from([("input2".into(), "mock-{input1}".into())]),
        temp.path(),
    )
    .unwrap();
    assert_eq!(spec["url"], "https://example.test/demo/mcp");
    assert_eq!(spec["headers"]["Authorization"], "Bearer mock-{input1}");
    assert_eq!(
        recipes[1].0.optional_args["input2"],
        vec!["--limit", "{input2}"]
    );
    assert_eq!(recipes[1].0.args, vec!["--verbose"]);
    assert_eq!(recipes[1].1[0].choices, vec!["read", "write"]);
    assert_eq!(
        process::expand_template("{unknown}", &BTreeMap::new(), "data"),
        "{unknown}"
    );
}
#[test]
fn source_documents_extract_configs_but_never_shell_commands() {
    let readme = "# Install\n```sh\ncurl https://example.test/setup | sh\n```\n```json\n{\"mcpServers\":{\"demo\":{\"command\":\"npx\",\"args\":[\"-y\",\"@example/mcp@1.2.3\",\"--mode\",\"read\"],\"env\":{\"TOKEN\":\"YOUR_TOKEN\"}}}}\n```";
    let configs = sources::document_configs(readme);
    assert_eq!(configs.len(), 1);
    let (r, fields) = sources::config_recipe(&configs[0]).unwrap();
    assert_eq!(r.package, "@example/mcp");
    assert_eq!(r.version, "1.2.3");
    assert_eq!(r.args, vec!["--mode", "read"]);
    assert_eq!(fields[0].kind, "secret");
    assert!(fields[0].required);
    assert_eq!(r.env["TOKEN"], "{input1}");
    let (r, _) =
        sources::config_recipe(&json!({"command":"uvx","args":["mcp-server-time==2025.8.4"]}))
            .unwrap();
    assert_eq!(r.package, "mcp-server-time");
    assert_eq!(r.kind, "pypi");
    assert!(sources::config_recipe(&json!({"command":"cmd","args":["/c","echo hi"]})).is_err());
    assert!(
        sources::config_recipe(&json!({"command":"npx","args":["--package=other","server"]}))
            .is_err()
    );
    assert!(sources::config_recipe(
        &json!({"command":"npx","args":["git+https://example.test/repo"]})
    )
    .is_err());
    assert!(sources::config_recipe(&json!({"url":"https://example.test","command":"sh"})).is_err());
}
#[test]
fn source_rejects_paths_runtime_options_and_unsupported_integrity() {
    for name in [
        "../../x",
        "--config=x",
        "file:package",
        "https://example.test",
        "@scope/../../x",
        "x;echo",
    ] {
        assert!(
            sources::validate_package("npm", name, "1.0.0").is_err(),
            "{name}"
        );
    }
    for bin in ["../x.js", "C:/x.js", "/x.js", "dist/../../x.js"] {
        assert!(sources::validate_bin("npm", bin).is_err());
    }
    assert!(sources::validate_bin("npm", "./dist/index.js").is_ok());
    let mut package = json!({"registryType":"npm","identifier":"valid","version":"1.0.0","transport":{"type":"stdio"}});
    package["runtimeArguments"] = json!([{"value":"--require=evil"}]);
    assert!(
        sources::registry_recipes(&json!({"packages":[package.clone()]}))
            .0
            .is_empty()
    );
    package["runtimeArguments"] = json!([]);
    package["fileSha256"] = json!("0".repeat(64));
    assert!(sources::registry_recipes(&json!({"packages":[package]}))
        .0
        .is_empty());
}
fn mock_installation(path: &Path) -> Installation {
    Installation {
        key: "codex:demo".into(),
        id: "demo".into(),
        name: "Demo".into(),
        agent: "codex".into(),
        path: path.join("agent.json"),
        enabled: false,
        spec: json!({"command":path.join("demo/1.0.0/server.exe")}),
        target: json!({}),
        values: BTreeMap::new(),
        entry: None,
        version: "1.0.0".into(),
        check: String::new(),
    }
}
#[test]
fn uninstall_preserves_disabled_shared_packages_external_refs_and_data() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let path = root.join("demo/1.0.0");
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(
        path.join("mcp-install.json"),
        json!({"package":"demo","version":"1.0.0"}).to_string(),
    )
    .unwrap();
    std::fs::create_dir_all(root.join("demo/data")).unwrap();
    std::fs::write(root.join("demo/data/keep.txt"), "mock-data").unwrap();
    let ledger = Ledger {
        installations: vec![mock_installation(root)],
        ..Ledger::default()
    };
    let never = |_: &Path| -> Result<(), String> { panic!("must not recycle referenced package") };
    assert!(
        uninstall::cleanup_with(root, &ledger, "demo", "1.0.0", &[], never)
            .unwrap_err()
            .contains("其他 Agent")
    );
    let external = json!({"args":[format!("--path={}/server.js",path.to_string_lossy().to_uppercase().replace('\\',"/"))]});
    assert!(uninstall::cleanup_with(
        root,
        &Ledger::default(),
        "demo",
        "1.0.0",
        &[external],
        never
    )
    .unwrap_err()
    .contains("引用"));
    uninstall::cleanup_with(root, &Ledger::default(), "demo", "1.0.0", &[], |p| {
        assert_eq!(p, path);
        std::fs::remove_dir_all(p).map_err(|e| e.to_string())
    })
    .unwrap();
    assert!(root.join("demo/data/keep.txt").exists());
    assert!(!path.exists());
}
#[test]
fn uninstall_rejects_traversal_missing_marker_and_reports_file_failures() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let ledger = Ledger::default();
    let never = |_: &Path| -> Result<(), String> { panic!("invalid removal") };
    for (id, v) in [
        ("../demo", "1"),
        ("demo", ".."),
        ("demo", "data"),
        ("demo", "1.0"),
    ] {
        assert!(uninstall::cleanup_with(root, &ledger, id, v, &[], never).is_err());
    }
    let path = root.join("demo/1.0");
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(
        path.join("mcp-install.json"),
        json!({"package":"demo","version":"1.0"}).to_string(),
    )
    .unwrap();
    assert_eq!(
        uninstall::cleanup_with(root, &ledger, "demo", "1.0", &[], |_| Err(
            "mock locked".into()
        ))
        .unwrap_err(),
        "mock locked"
    );
    assert!(path.exists());
    assert!(
        uninstall::cleanup_with(root, &ledger, "demo", "1.0", &[], |_| Ok(()))
            .unwrap_err()
            .contains("仍被占用")
    );
}
#[tokio::test]
#[ignore = "Reads live Registry and GitHub metadata; installs one npm/Python package into temp directories"]
async fn live_source_resolution_and_install() {
    let rows = catalog::registry_search("memory").await.unwrap();
    let mut found = false;
    for row in rows.iter().take(4) {
        let result = sources::resolve("registry", &row.id).await.unwrap();
        println!(
            "registry {}: {} plans {:?}",
            row.id,
            result.plans.len(),
            result.notes
        );
        if !result.plans.is_empty() {
            found = true;
            break;
        }
    }
    assert!(found, "Official Registry should yield a supported plan");
    let directory: Vec<Entry> =
        serde_json::from_str(include_str!("../../../src/mcp/community.json")).unwrap();
    let entry = directory
        .iter()
        .find(|e| e.homepage == "https://github.com/jae-jae/fetcher-mcp")
        .unwrap();
    let result = sources::resolve("awesome", &entry.id).await.unwrap();
    println!(
        "community {}: {} plans {:?}",
        entry.id,
        result.plans.len(),
        result.notes
    );
    assert!(!result.plans.is_empty());
    let temp = tempfile::tempdir().unwrap();
    let r=sources::pin(serde_json::from_value(json!({"kind":"npm","package":"@modelcontextprotocol/server-sequential-thinking","version":"2025.7.1"})).unwrap()).await.unwrap();
    let mut entry = catalog::template("sequential-thinking").unwrap();
    entry.recipe = Some(r);
    let spec = process::build_spec(&entry, &BTreeMap::new(), temp.path()).unwrap();
    assert!(process::probe_stdio(&spec).unwrap().contains("通过"));
}
#[tokio::test]
#[ignore = "Installs a real Python MCP in a temporary directory"]
async fn resolved_python_entry_point() {
    let temp = tempfile::tempdir().unwrap();
    let r = sources::pin(
        serde_json::from_value(
            json!({"kind":"pypi","package":"mcp-server-time","version":"2026.8.18"}),
        )
        .unwrap(),
    )
    .await
    .unwrap();
    let mut entry = catalog::template("time").unwrap();
    entry.recipe = Some(r);
    let spec = process::build_spec(&entry, &BTreeMap::new(), temp.path()).unwrap();
    assert!(process::probe_stdio(&spec).unwrap().contains("通过"));
}

#[test]
fn source_registry_does_not_change_package_registries_or_assume_oauth() {
    let (recipes, notes) = sources::registry_recipes(
        &json!({"packages":[{"registryType":"npm","registryBaseUrl":"https://pypi.org","identifier":"demo","version":"1.0.0","transport":{"type":"stdio"}}],"remotes":[{"type":"streamable-http","url":"https://example.test/mcp"}]}),
    );
    assert_eq!(recipes.len(), 1);
    assert!(!recipes[0].0.oauth);
    assert_eq!(notes.len(), 1);
    let (recipe, fields) = sources::config_recipe(
        &json!({"command":"npx","args":["filesystem","/path/to/allowed/folder"]}),
    )
    .unwrap();
    assert_eq!(fields.len(), 1);
    assert!(fields[0].required);
    assert_eq!(recipe.args, vec!["{input1}"]);
}
