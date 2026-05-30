import urllib.request
import json
import requests
import pandas as pd

CODE = "603629"
PREFIX = "sh"
UA = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36"

# ===== 1. 腾讯财经实时行情 =====
url = f"https://qt.gtimg.cn/q={PREFIX}{CODE}"
req = urllib.request.Request(url)
req.add_header("User-Agent", "Mozilla/5.0")
resp = urllib.request.urlopen(req, timeout=10)
data = resp.read().decode("gbk")
vals = data.split('"')[1].split("~")
print("=== 腾讯实时行情 ===")
print(f"名称: {vals[1]}")
print(f"代码: {CODE}")
print(f"当前价: {vals[3]}")
print(f"昨收: {vals[4]}")
print(f"今开: {vals[5]}")
print(f"最高: {vals[33]}")
print(f"最低: {vals[34]}")
print(f"涨跌额: {vals[31]}")
print(f"涨跌幅: {vals[32]}%")
print(f"换手率: {vals[38]}%")
print(f"PE(TTM): {vals[39]}")
print(f"PB: {vals[46]}")
print(f"总市值: {vals[44]}亿")
print(f"流通市值: {vals[45]}亿")
print(f"涨停价: {vals[47]}")
print(f"跌停价: {vals[48]}")
print(f"振幅: {vals[43]}%")
print(f"量比: {vals[49]}")

# ===== 2. 东财个股基本面信息 =====
print("\n=== 东财个股基本面 ===")
mkt = 1 if CODE.startswith("6") else 0
url2 = "https://push2.eastmoney.com/api/qt/stock/get"
params = {"fltt":"2","invt":"2","fields":"f57,f58,f84,f85,f127,f116,f117,f189,f43","secid":f"{mkt}.{CODE}"}
r = requests.get(url2, params=params, headers={"User-Agent": UA}, timeout=10)
d = r.json().get("data", {})
print(f"行业: {d.get('f127','')}")
print(f"总股本: {d.get('f84',0)} 股")
print(f"流通股: {d.get('f85',0)} 股")
print(f"上市日期: {d.get('f189','')}")
print(f"最新收盘: {d.get('f43',0)}")

# ===== 3. 百度概念板块归属 =====
print("\n=== 百度概念板块归属 ===")
url3 = f"https://finance.pae.baidu.com/api/getrelatedblock?code={CODE}&market=ab&typeCode=all&finClientType=pc"
r3 = requests.get(url3, headers={
    "Host": "finance.pae.baidu.com", "User-Agent": UA,
    "Accept": "application/vnd.finance-web.v1+json",
    "Origin": "https://gushitong.baidu.com", "Referer": "https://gushitong.baidu.com/"
}, timeout=10)
d3 = r3.json()
if str(d3.get("ResultCode", -1)) == "0":
    for block in d3.get("Result", []):
        bt = block.get("type", "")
        items = [item.get("name","") + "("+str(item.get("increase",""))+"%)" for item in block.get("list",[])]
        print(f"{bt}: {', '.join(items[:10])}")
else:
    print(f"错误: {d3}")

# ===== 4. 同花顺一致预期EPS =====
print("\n=== 同花顺一致预期EPS ===")
try:
    url4 = f"https://basic.10jqka.com.cn/new/{CODE}/worth.html"
    r4 = requests.get(url4, headers={"User-Agent": UA, "Referer": "https://basic.10jqka.com.cn/"}, timeout=15)
    r4.encoding = "gbk"
    dfs = pd.read_html(r4.text)
    found = False
    for df in dfs:
        cols = [str(c) for c in df.columns]
        if any("每股收益" in c or "均值" in c or "预测" in c for c in cols):
            print(df.to_string())
            found = True
            break
    if not found and dfs:
        print(dfs[0].to_string())
except Exception as e:
    print(f"无机构覆盖数据: {e}")

# ===== 5. 东财研报 =====
print("\n=== 东财研报列表 ===")
url5 = "https://reportapi.eastmoney.com/report/list"
params5 = {"pageSize":"20","pageNo":"1","code":CODE,
           "industryCode":"*","industry":"*","rating":"*","ratingChange":"*",
           "beginTime":"2025-01-01","endTime":"2030-01-01",
           "fields":"","qType":"0","orgCode":"","rcode":""}
r5 = requests.get(url5, params=params5, headers={"User-Agent": UA, "Referer": "https://data.eastmoney.com/"}, timeout=30)
d5 = r5.json()
reports = d5.get("data") or []
print(f"共 {len(reports)} 篇研报")
for rep in reports[:10]:
    title = rep.get('title','')[:50]
    date = rep.get('publishDate','')[:10]
    org = rep.get('orgSName','')
    eps_this = rep.get('predictThisYearEps','')
    eps_next = rep.get('predictNextYearEps','')
    eps_nn = rep.get('predictNextTwoYearEps','')
    rating = rep.get('emRatingName','')
    print(f"  {date} | {org} | {title}")
    if eps_this:
        print(f"    评级:{rating}  EPS:今年{eps_this} 明年{eps_next} 后年{eps_nn}")
