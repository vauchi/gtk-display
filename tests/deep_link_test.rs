// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! The vauchi.app/x landing page's "Open in Vauchi" button and every shared
//! `vauchi://` link must reach the installed app (vauchi/private#650): the
//! shell passes only `vauchi://` links to Core. linux-app tests the desktop
//! entry that registers the scheme.

use vauchi_gtk::platform::deep_link::deep_link_uris;

// @internal
#[test]
fn only_vauchi_links_are_passed_to_core() {
    let opened = [
        "vauchi://exchange?pk=AAAA&n=BBBB".to_owned(),
        "file:///home/someone/contacts.vcf".to_owned(),
        "https://vauchi.app/x#pk=AAAA&n=BBBB".to_owned(),
        "VAUCHI://exchange?pk=CCCC&n=DDDD".to_owned(),
        "vauchi-evil://exchange".to_owned(),
    ];

    assert_eq!(
        deep_link_uris(&opened),
        [
            "vauchi://exchange?pk=AAAA&n=BBBB",
            "VAUCHI://exchange?pk=CCCC&n=DDDD"
        ]
    );
}
