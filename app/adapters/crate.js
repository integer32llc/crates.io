import RESTAdapter from '@ember-data/adapter/rest';
import { sanitizeSubcrateIdForUrl } from '../utils/subcrate';

export default class CrateAdapter extends RESTAdapter {
  namespace = 'api/v1';
  buildURL(modelName, id, snapshot, requestType, query) {
    var sanitizedId = sanitizeSubcrateIdForUrl(id);
    return super.buildURL(modelName, sanitizedId, snapshot, requestType, query);
  }
}
