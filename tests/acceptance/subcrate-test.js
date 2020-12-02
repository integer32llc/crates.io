import { click, currentURL, currentRouteName, visit, waitFor } from '@ember/test-helpers';
import { setupApplicationTest } from 'ember-qunit';
import { module, test, skip } from 'qunit';

import percySnapshot from '@percy/ember';
import a11yAudit from 'ember-a11y-testing/test-support/audit';

import axeConfig from '../axe-config';
import { title } from '../helpers/dom';
import setupMirage from '../helpers/setup-mirage';

module('Acceptance | crate page for a subcrate', function (hooks) {
  setupApplicationTest(hooks);
  setupMirage(hooks);

  test('visiting a crate page from the front page', async function (assert) {
    this.server.create('crate', { name: 'serde/json', newest_version: '0.6.1' });
    this.server.create('version', { crateId: 'serde/json', num: '0.6.1' });

    await visit('/');
    await click('[data-test-just-updated] [data-test-crate-link="0"]');

    assert.equal(currentURL(), '/crates/serde~json');
    assert.equal(title(), 'serde/json - crates.io: Rust Package Registry');

    assert.dom('[data-test-heading] [data-test-crate-name]').hasText('serde/json');
    assert.dom('[data-test-heading] [data-test-crate-version]').hasText('0.6.1');
  });

  test('visiting /crates/serde~json', async function (assert) {
    this.server.create('crate', { name: 'serde/json' });
    this.server.create('version', { crateId: 'serde/json', num: '0.6.0' });
    this.server.create('version', { crateId: 'serde/json', num: '0.6.1' });

    await visit('/crates/serde~json');

    assert.equal(currentURL(), '/crates/serde~json');
    assert.equal(currentRouteName(), 'crate.index');
    assert.equal(title(), 'serde/json - crates.io: Rust Package Registry');

    assert.dom('[data-test-heading] [data-test-crate-name]').hasText('serde/json');
    assert.dom('[data-test-heading] [data-test-crate-version]').hasText('0.6.1');
    assert.dom('[data-test-crate-stats-label]').hasText('Stats Overview');

    await percySnapshot(assert);
    await a11yAudit(axeConfig);
  });

  test('visiting /crates/serde~json/', async function (assert) {
    this.server.create('crate', { name: 'serde/json' });
    this.server.create('version', { crateId: 'serde/json', num: '0.6.0' });
    this.server.create('version', { crateId: 'serde/json', num: '0.6.1' });

    await visit('/crates/serde~json/');

    assert.equal(currentURL(), '/crates/serde~json/');
    assert.equal(currentRouteName(), 'crate.index');
    assert.equal(title(), 'serde/json - crates.io: Rust Package Registry');

    assert.dom('[data-test-heading] [data-test-crate-name]').hasText('serde/json');
    assert.dom('[data-test-heading] [data-test-crate-version]').hasText('0.6.1');
    assert.dom('[data-test-crate-stats-label]').hasText('Stats Overview');
  });

  test('visiting /crates/serde~json/0.6.0', async function (assert) {
    this.server.create('crate', { name: 'serde/json' });
    this.server.create('version', { crateId: 'serde/json', num: '0.6.0' });
    this.server.create('version', { crateId: 'serde/json', num: '0.6.1' });

    await visit('/crates/serde~json/0.6.0');

    assert.equal(currentURL(), '/crates/serde~json/0.6.0');
    assert.equal(currentRouteName(), 'crate.version');
    assert.equal(title(), 'serde/json - crates.io: Rust Package Registry');

    assert.dom('[data-test-heading] [data-test-crate-name]').hasText('serde/json');
    assert.dom('[data-test-heading] [data-test-crate-version]').hasText('0.6.0');
    assert.dom('[data-test-crate-stats-label]').hasText('Stats Overview for 0.6.0 (see all)');

    await percySnapshot(assert);
    await a11yAudit(axeConfig);
  });

  test('unknown versions fall back to latest version and show an error message', async function (assert) {
    this.server.create('crate', { name: 'serde/json' });
    this.server.create('version', { crateId: 'serde/json', num: '0.6.0' });
    this.server.create('version', { crateId: 'serde/json', num: '0.6.1' });

    await visit('/crates/serde~json/0.7.0');

    assert.equal(currentURL(), '/crates/serde~json/0.7.0');
    assert.dom('[data-test-heading] [data-test-crate-name]').hasText('serde/json');
    assert.dom('[data-test-heading] [data-test-crate-version]').hasText('0.6.1');
    assert.dom('[data-test-notification-message]').hasText("Version '0.7.0' of crate 'serde/json' does not exist");
  });

});
