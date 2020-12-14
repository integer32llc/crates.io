import Route from '@ember/routing/route';

export default class OwnersRoute extends Route {
  setupController(controller) {
    super.setupController(...arguments);
    let crate = this.modelFor('crate');
    crate.reload().then(function(crate) {
      controller.set('crate', crate);
    });
  }
}
