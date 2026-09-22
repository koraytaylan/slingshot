//! Which paths are editable page templates, and which are content fragment models.
//!
//! The rule is the parent folder, not a prefix. A node under a template is part of
//! that template, and a node whose name merely begins with `templates` is not one.

use slingshot_domain::command::find_pages_containing_phrase::PageMatch;
use slingshot_domain::command::list_content_fragment_models::{
    ListContentFragmentModelsCommand, ListContentFragmentModelsResult,
};
use slingshot_domain::command::list_page_templates::{
    ListPageTemplatesCommand, ListPageTemplatesResult,
};
use slingshot_domain::command::query_paths::DiscoveryResultFailure;
use slingshot_domain::command::repository_path::RepositoryPath;

fn path(value: &str) -> RepositoryPath {
    RepositoryPath::parse(value).expect("a legal path")
}

fn templates(root: &str) -> ListPageTemplatesCommand {
    ListPageTemplatesCommand { result_window: None, root_path: path(root) }
}

fn models(root: &str) -> ListContentFragmentModelsCommand {
    ListContentFragmentModelsCommand { result_window: None, root_path: path(root) }
}

fn page(repository_path: &str) -> PageMatch {
    PageMatch { repository_path: path(repository_path), title: None }
}

#[test]
fn a_template_is_the_node_inside_the_templates_folder_and_nothing_under_it() {
    let command = templates("/conf/site");
    let template = "/conf/site/settings/wcm/templates/article";
    assert!(command.admits(&path(template)));
    assert!(!command.admits(&path("/conf/site/settings/wcm/templates/article/jcr:content")));
    assert!(!command.admits(&path("/conf/site/settings/wcm/template-types/page")));
    assert!(!command.admits(&path("/conf/other/settings/wcm/templates/article")));
    let answered = ListPageTemplatesResult::new(vec![page(template)], None).expect("ordered");
    assert!(answered.require_answers(&command).is_ok());
    let elsewhere = ListPageTemplatesResult::new(
        vec![page("/conf/other/settings/wcm/templates/article")],
        None,
    )
    .expect("ordered");
    assert_eq!(elsewhere.require_answers(&command), Err(DiscoveryResultFailure::NotThisRequest));
}

#[test]
fn a_model_is_the_node_inside_the_models_folder() {
    let command = models("/conf");
    let model = "/conf/site/settings/dam/cfm/models/article";
    assert!(command.admits(&path(model)));
    assert!(!command.admits(&path("/conf/site/settings/dam/cfm/models/article/jcr:content")));
    assert!(!command.admits(&path("/conf/site/settings/wcm/templates/article")));
    let answered = ListContentFragmentModelsResult::new(vec![page(model)], None).expect("ordered");
    assert!(answered.require_answers(&command).is_ok());
}
