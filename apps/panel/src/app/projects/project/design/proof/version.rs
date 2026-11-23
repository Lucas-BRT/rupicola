//! `/projects/{project}/design/proof/{version}`

mod proof_name;

use topcoat::router::module_param;

module_param!(pub version);
