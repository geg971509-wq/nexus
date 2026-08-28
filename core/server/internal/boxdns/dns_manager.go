package boxdns

import "github.com/sagernet/sing/common/control"

// HandleSystemDNS is a no-op because macOS DNS changes are handled through
// sys.SetSystemDNS. The always-on interface monitor still registers this
// callback so DefaultInterface behaves consistently.
func (d *DnsManager) HandleSystemDNS(ifc *control.Interface, flag int) {}
