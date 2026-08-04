#include "include/configs/common/OutboundFactory.h"

#include "include/configs/outbounds/socks.h"
#include "include/configs/outbounds/http.h"
#include "include/configs/outbounds/shadowsocks.h"
#include "include/configs/outbounds/chain.h"
#include "include/configs/outbounds/autoselector.h"
#include "include/configs/outbounds/vmess.h"
#include "include/configs/outbounds/trojan.h"
#include "include/configs/outbounds/vless.h"
#include "include/configs/outbounds/xrayVless.h"
#include "include/configs/outbounds/hysteria.h"
#include "include/configs/outbounds/tuic.h"
#include "include/configs/outbounds/juicity.h"
#include "include/configs/outbounds/trusttunnel.h"
#include "include/configs/outbounds/anyTLS.h"
#include "include/configs/outbounds/mieru.h"
#include "include/configs/outbounds/shadowtls.h"
#include "include/configs/outbounds/wireguard.h"
#include "include/configs/outbounds/tailscale.h"
#include "include/configs/outbounds/ssh.h"
#include "include/configs/outbounds/custom.h"
#include "include/configs/outbounds/extracore.h"
#include "include/configs/outbounds/naive.h"
#include "include/configs/outbounds/direct.h"

namespace Configs
{
    std::shared_ptr<outbound> NewOutboundByType(const QString& type)
    {
        if (type == "socks") return std::make_shared<socks>();
        if (type == "http") return std::make_shared<http>();
        if (type == "shadowsocks") return std::make_shared<shadowsocks>();
        if (type == "chain") return std::make_shared<chain>();
        if (type == "autoselector") return std::make_shared<autoSelector>();
        if (type == "vmess") return std::make_shared<vmess>();
        if (type == "trojan") return std::make_shared<Trojan>();
        if (type == "vless") return std::make_shared<vless>();
        if (type == "xrayvless") return std::make_shared<xrayVless>();
        if (type == "hysteria" || type == "hysteria2") return std::make_shared<hysteria>();
        if (type == "tuic") return std::make_shared<tuic>();
        if (type == "juicity") return std::make_shared<juicity>();
        if (type == "trusttunnel") return std::make_shared<trusttunnel>();
        if (type == "anytls") return std::make_shared<anyTLS>();
        if (type == "mieru") return std::make_shared<mieru>();
        if (type == "shadowtls") return std::make_shared<shadowtls>();
        if (type == "wireguard") return std::make_shared<wireguard>();
        if (type == "tailscale") return std::make_shared<tailscale>();
        if (type == "ssh") return std::make_shared<ssh>();
        if (type == "custom") return std::make_shared<Custom>();
        if (type == "extracore") return std::make_shared<extracore>();
        if (type == "naive") return std::make_shared<naive>();
        if (type == "direct") return std::make_shared<direct>();
        auto ob = std::make_shared<outbound>();
        ob->invalid = true;
        return ob;
    }
}
