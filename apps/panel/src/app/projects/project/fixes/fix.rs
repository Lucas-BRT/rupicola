//! `/projects/{project}/fixes/{fix}`

mod review;

use topcoat::router::module_param;

module_param!(pub fix);
