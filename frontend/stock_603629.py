import urllib.request

code = "603629"
url = "https://qt.gtimg.cn/q=sh" + code
req = urllib.request.Request(url)
req.add_header("User-Agent", "Mozilla/5.0")
resp = urllib.request.urlopen(req, timeout=10)
data = resp.read().decode("gbk")
vals = data.split('"')[1].split("~")

name = vals[1]
price = float(vals[3]) if vals[3] else 0
last_close = float(vals[4]) if vals[4] else 0
open_p = float(vals[5]) if vals[5] else 0
change_amt = float(vals[31]) if vals[31] else 0
change_pct = float(vals[32]) if vals[32] else 0
high = float(vals[33]) if vals[33] else 0
low = float(vals[34]) if vals[34] else 0
amount_wan = float(vals[37]) if vals[37] else 0
turnover_pct = float(vals[38]) if vals[38] else 0
pe_ttm = float(vals[39]) if vals[39] else 0
amplitude_pct = float(vals[43]) if vals[43] else 0
mcap_yi = float(vals[44]) if vals[44] else 0
float_mcap_yi = float(vals[45]) if vals[45] else 0
pb = float(vals[46]) if vals[46] else 0
limit_up = float(vals[47]) if vals[47] else 0
limit_down = float(vals[48]) if vals[48] else 0
vol_ratio = float(vals[49]) if vals[49] else 0
pe_static = float(vals[52]) if vals[52] else 0

print(f"=== {name}({code}) 实时行情 ===")
print(f"当前价: {price} 元")
print(f"涨跌幅: {change_pct}% ({change_amt}元)")
print(f"今开: {open_p}  昨收: {last_close}")
print(f"最高: {high}  最低: {low}")
print(f"振幅: {amplitude_pct}%")
print(f"成交额: {amount_wan:.0f} 万元")
print(f"换手率: {turnover_pct}%")
print(f"量比: {vol_ratio}")
print(f"涨停价: {limit_up}  跌停价: {limit_down}")
print(f"PE(TTM): {pe_ttm}  PE(静): {pe_static}")
print(f"PB: {pb}")
print(f"总市值: {mcap_yi:.2f} 亿")
print(f"流通市值: {float_mcap_yi:.2f} 亿")
