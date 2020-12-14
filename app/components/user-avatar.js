import { readOnly } from '@ember/object/computed';
import Component from '@glimmer/component';

export default class UserAvatar extends Component {
  get width() {
    if (this.args.size === 'medium') {
      return 85;
    } else if (this.args.size === 'medium-small') {
      return 32;
    } else {
      return 22; // small
    }
  }

  @readOnly('width') height;

  get alt() {
    return `${this.args.user.name} (${this.args.user.login})`;
  }

  get title() {
    let user = this.args.user;

    switch (user.kind) {
      case 'user':
        return user.name;
      case 'namespace_user':
        return `${user.name} (namespace owner)`;
      case 'team':
        return `${user.name} team`;
      case 'namespace_team':
        return `${user.name} team (namespace owner)`;
      default:
        return `${user.name} (${user.kind})`;
    }
  }

  get src() {
    return `${this.args.user.avatar}&s=${this.width * 2}`;
  }
}
