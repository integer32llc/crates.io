import Model, { belongsTo, attr } from '@ember-data/model';

import Inflector from 'ember-inflector';
import { sanitizeSubcrateIdForUrl } from '../utils/subcrate';

Inflector.inflector.irregular('dependency', 'dependencies');

export default class Dependency extends Model {
  @attr crate_id;
  @attr req;
  @attr optional;
  @attr default_features;
  @attr({ defaultValue: () => [] }) features;
  @attr kind;
  @attr downloads;

  get fileSafeCrateId() {
    return sanitizeSubcrateIdForUrl(this.crate_id);
  }

  @belongsTo('version', { async: false }) version;
}
