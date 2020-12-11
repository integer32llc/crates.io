import Model, { attr } from '@ember-data/model';
import { sanitizeSubcrateIdForUrl } from '../utils/subcrate';

export default class CrateOwnerInvite extends Model {
  @attr invited_by_username;
  @attr crate_name;
  @attr crate_id;
  @attr('date') created_at;
  @attr accepted;

  get fileSafeCrateId() {
    return sanitizeSubcrateIdForUrl(this.crate_name);
  }
}
