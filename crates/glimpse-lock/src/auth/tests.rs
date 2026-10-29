use super::*;

fn refused(message: Message, shake: bool) -> Verdict {
    Verdict::Refused { message, shake }
}

#[test]
fn success_unlocks() {
    assert_eq!(
        classify(Ok(()), None),
        Verdict::Unlock {
            password_expired: false
        }
    );
}

#[test]
fn a_rejection_without_pam_text_is_a_wrong_password() {
    assert_eq!(
        classify(Err((Step::Authenticate, ErrorCode::AUTH_ERR)), None),
        refused(Message::WrongPassword, true)
    );
    assert_eq!(
        classify(Err((Step::Authenticate, ErrorCode::USER_UNKNOWN)), None),
        refused(Message::WrongPassword, true)
    );
}

#[test]
fn pam_text_wins_over_a_wrong_password() {
    let faillock = "The account is locked due to 3 failed logins.";
    assert_eq!(
        classify(
            Err((Step::Authenticate, ErrorCode::AUTH_ERR)),
            Some(faillock)
        ),
        refused(Message::Pam(faillock.into()), true)
    );
}

#[test]
fn too_many_tries_prefers_the_pam_text() {
    assert_eq!(
        classify(Err((Step::Authenticate, ErrorCode::MAXTRIES)), None),
        refused(Message::TooManyAttempts, true)
    );
    assert_eq!(
        classify(
            Err((Step::Authenticate, ErrorCode::MAXTRIES)),
            Some("locked")
        ),
        refused(Message::Pam("locked".into()), true)
    );
}

#[test]
fn authinfo_unavailable_names_the_check_and_is_never_a_wrong_password() {
    for text in [None, Some("anything")] {
        let verdict = classify(Err((Step::Authenticate, ErrorCode::AUTHINFO_UNAVAIL)), text);
        assert_eq!(verdict, refused(Message::CantVerify, false));
    }
    assert!(Message::CantVerify.text().contains("glimpse-lock check"));
}

#[test]
fn account_problems_prefer_the_pam_text() {
    for code in [ErrorCode::ACCT_EXPIRED, ErrorCode::PERM_DENIED] {
        assert_eq!(
            classify(Err((Step::Account, code)), None),
            refused(Message::AccountRefused, false)
        );
        assert_eq!(
            classify(Err((Step::Account, code)), Some("expired")),
            refused(Message::Pam("expired".into()), false)
        );
    }
}

#[test]
fn an_expired_password_after_a_correct_one_unlocks_and_says_so() {
    for code in [ErrorCode::NEW_AUTHTOK_REQD, ErrorCode::AUTHTOK_EXPIRED] {
        assert_eq!(
            classify(Err((Step::Account, code)), Some("change it")),
            Verdict::Unlock {
                password_expired: true
            }
        );
        assert_eq!(
            classify(Err((Step::Authenticate, code)), Some("change it")),
            refused(Message::PasswordChange, false),
            "only an account check after a successful authenticate unlocks"
        );
    }
    assert_eq!(
        classify(Err((Step::Account, ErrorCode::ACCT_EXPIRED)), None),
        refused(Message::AccountRefused, false),
        "an expired account still refuses"
    );
    assert_eq!(
        classify(Err((Step::Account, ErrorCode::SYSTEM_ERR)), None),
        refused(Message::Failed, false)
    );
}

#[test]
fn anything_else_is_a_generic_failure() {
    assert_eq!(
        classify(
            Err((Step::Authenticate, ErrorCode::SYSTEM_ERR)),
            Some("text")
        ),
        refused(Message::Failed, false)
    );
}

#[test]
fn the_conversation_answers_the_user_and_the_password_once() {
    let mut conversation = Conversation::new("sam", Zeroizing::new("hunter2".to_owned()));
    assert_eq!(
        conversation
            .prompt_echo_on(c"login:")
            .expect("user")
            .as_bytes(),
        b"sam"
    );
    assert_eq!(
        conversation
            .prompt_echo_off(c"Password:")
            .expect("password")
            .as_bytes(),
        b"hunter2"
    );
    assert_eq!(
        conversation.prompt_echo_off(c"OTP:").err(),
        Some(ErrorCode::CONV_ERR)
    );
}

#[test]
fn the_conversation_keeps_the_last_pam_text_sanitized_and_capped() {
    let mut conversation = Conversation::new("sam", Zeroizing::new(String::new()));
    assert_eq!(conversation.text(), None);
    conversation.text_info(c"first");
    conversation.error_msg(c"The account\tis \x1b[31mlocked\n");
    assert_eq!(conversation.text(), Some("The account is [31mlocked"));
    conversation.text_info(c"   ");
    assert_eq!(conversation.text(), Some("The account is [31mlocked"));
    let long = CString::new("x".repeat(1000)).expect("no nul");
    conversation.text_info(&long);
    assert_eq!(
        conversation.text().map(|text| text.chars().count()),
        Some(PAM_TEXT_MAX_CHARS + 1)
    );
}

#[test]
fn only_the_ordinary_refusals_are_cleared_when_the_cover_returns() {
    assert!(!Message::WrongPassword.lasting());
    assert!(!Message::TimedOut.lasting());
    assert!(!Message::Failed.lasting());
    assert!(Message::Pam("(10 minutes left to unlock)".to_owned()).lasting());
    assert!(Message::TooManyAttempts.lasting());
    assert!(Message::AccountRefused.lasting());
    assert!(Message::PasswordChange.lasting());
    assert!(Message::CantVerify.lasting());
    assert!(Message::ConsoleOnly.lasting());
}
