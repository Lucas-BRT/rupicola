//! `/projects/{project}/problems/{problem}`

mod triage;

use topcoat::router::module_param;

module_param!(pub problem);
