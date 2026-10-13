// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TestOpRef {
    pub(super) path: String,
}

impl AuthCredentialRef for TestOpRef {
    fn path(&self) -> &str {
        &self.path
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TestCredential {
    Plain(String),
    OpRef(TestOpRef),
}

impl AuthCredential for TestCredential {
    type Ref = TestOpRef;

    fn into_credential_input(self) -> CredentialInput<Self::Ref> {
        match self {
            Self::Plain(value) => CredentialInput::Literal(value),
            Self::OpRef(value) => CredentialInput::OpRef(value),
        }
    }

    fn from_plain(value: String) -> Self {
        Self::Plain(value)
    }

    fn from_op_ref(value: Self::Ref) -> Self {
        Self::OpRef(value)
    }
}

pub(super) type TestForm = AuthForm<TestCredential>;

pub(super) fn dump_form(form: &TestForm) -> String {
    let backend = TestBackend::new(100, 20);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|frame| {
        let area = frame.area();
        render_form(frame, area, form, AuthFormFocus::Mode);
    })
    .unwrap();
    let buf = term.backend().buffer();
    let mut output = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            output.push_str(buf[(x, y)].symbol());
        }
        output.push('\n');
    }
    output
}
