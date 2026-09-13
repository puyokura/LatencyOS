use pulselang_core::compile;

#[test]
fn test_resource_valid_move_and_release() {
    let src = r#"
        #f0 := @capture();
        #f1 := #f0;
        @release(#f1);
    "#;
    let bin = compile(src).expect("Move ownership and release must compile cleanly");
    assert!(bin.len() > 16);
}

#[test]
fn test_resource_valid_move_and_send() {
    let src = r#"
        #f0 := @capture();
        #f1 := #f0;
        @send(#f1);
    "#;
    let bin = compile(src).expect("Move ownership and send must compile cleanly");
    assert!(bin.len() > 16);
}

#[test]
fn test_resource_use_after_move_rejected() {
    let src1 = r#"
        #f0 := @capture();
        #f1 := #f0;
        @send(#f0);
    "#;
    let err1 = compile(src1).unwrap_err();
    assert_eq!(err1.code, "ERR_RESOURCE_USE_AFTER_MOVE");

    let src2 = r#"
        #f0 := @capture();
        #f1 := #f0;
        #f2 := #f0;
    "#;
    let err2 = compile(src2).unwrap_err();
    assert_eq!(err2.code, "ERR_RESOURCE_USE_AFTER_MOVE");
}

#[test]
fn test_resource_double_release_rejected() {
    let src = r#"
        #f0 := @capture();
        @release(#f0);
        @release(#f0);
    "#;
    let err = compile(src).unwrap_err();
    assert_eq!(err.code, "ERR_RESOURCE_DOUBLE_RELEASE");
}

#[test]
fn test_resource_double_send_rejected() {
    let src = r#"
        #f0 := @capture();
        @send(#f0);
        @send(#f0);
    "#;
    let err = compile(src).unwrap_err();
    assert_eq!(err.code, "ERR_LINEAR_DOUBLE_SEND");
}

#[test]
fn test_resource_unconsumed_leak_rejected() {
    let src = r#"
        #f0 := @capture();
    "#;
    let err = compile(src).unwrap_err();
    assert_eq!(err.code, "ERR_LINEAR_UNCONSUMED_HANDLE");
}

#[test]
fn test_resource_loop_confinement_rejected() {
    let src = r#"
        for $i in 0..5 {
            #f0 := @capture();
        }
    "#;
    let err = compile(src).unwrap_err();
    assert_eq!(err.code, "ERR_TYPESTATE_MISMATCH");
}
