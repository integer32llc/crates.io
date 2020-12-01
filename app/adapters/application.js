import RESTAdapter from '@ember-data/adapter/rest';
import { inject as service } from '@ember/service';
import sanitizeSubcrateIdForUrl from '../utils/subcrate';

export default class ApplicationAdapter extends RESTAdapter {
  @service fastboot;

  namespace = 'api/v1';

  get headers() {
    if (this.fastboot.isFastBoot) {
      return { 'User-Agent': this.fastboot.request.headers.get('User-Agent') };
    }

    return {};
  }

  handleResponse(status, headers, payload, requestData) {
    if (typeof payload === 'string') {
      try {
        payload = JSON.parse(payload);
      } catch {
        // if the payload can't be parsed as JSON then let's continue
        // with the string payload
      }
    }

    return super.handleResponse(status, headers, payload, requestData);
  }

  buildURL(modelName, id, snapshot, requestType, query) {
    var sanitizedId = id;
    if (modelName == "crate") {
      sanitizedId = sanitizeSubcrateIdForUrl(id);
    }
    return super.buildURL(modelName, sanitizedId, snapshot, requestType, query);
  }
}
