import Route from '@ember/routing/route';
import { inject as service } from '@ember/service';
import { decodeSubcrateIdFromUrl } from '../utils/subcrate';

export default class CrateRoute extends Route {
  @service notifications;

  async model(params) {
    let crateId = decodeSubcrateIdFromUrl(params.crate_id);
    try {
      return await this.store.find('crate', crateId);
    } catch (error) {
      if (error.errors?.some(e => e.detail === 'Not Found')) {
        this.notifications.error(`Crate '${crateId}' does not exist`);
        this.replaceWith('index');
      } else {
        throw error;
      }
    }
  }

  afterModel(model) {
    if (model && typeof model.get === 'function') {
      this.setHeadTags(model);
    }
  }

  setHeadTags(model) {
    let headTags = [
      {
        type: 'meta',
        tagId: 'meta-description-tag',
        attrs: {
          name: 'description',
          content: model.get('description') || 'A package for Rust.',
        },
      },
    ];

    this.set('headTags', headTags);
  }
}
