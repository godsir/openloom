use openloom_models::Intent;
use regex::Regex;

pub struct KeywordRule {
    pub pattern: Regex,
    pub intent: Intent,
    pub confidence: f32,
}

pub fn default_keyword_rules() -> Vec<KeywordRule> {
    vec![
        // File operations
        KeywordRule {
            pattern: Regex::new(r"(?i)(打开|读取|写入|保存|删除|创建|新建|列出|查看).*(文件|文档|目录|文件夹)").unwrap(),
            intent: Intent::FileOperation,
            confidence: 0.90,
        },
        KeywordRule {
            pattern: Regex::new(r"(?i)(\.rs|\.py|\.js|\.ts|\.toml|\.json|\.md)\b").unwrap(),
            intent: Intent::FileOperation,
            confidence: 0.85,
        },
        // Web search
        KeywordRule {
            pattern: Regex::new(r"(?i)(搜索|查找|查询|百度|Google|搜一下|查一下)").unwrap(),
            intent: Intent::WebSearch,
            confidence: 0.90,
        },
        KeywordRule {
            pattern: Regex::new(r"(?i)(今天|最近|新闻|天气|最新)").unwrap(),
            intent: Intent::WebSearch,
            confidence: 0.75,
        },
        // Code assist
        KeywordRule {
            pattern: Regex::new(r"(?i)(写|编写|实现|修复|debug|重构|review|优化).*(代码|函数|方法|类|模块|脚本|程序)").unwrap(),
            intent: Intent::CodeAssist,
            confidence: 0.90,
        },
        KeywordRule {
            pattern: Regex::new(r"(?i)(代码|编译|运行|测试|单元测试|cargo|npm|pip|git|commit)").unwrap(),
            intent: Intent::CodeAssist,
            confidence: 0.85,
        },
        KeywordRule {
            pattern: Regex::new(r"(?i)(bug|错误|报错|失败|不对|不行|有问题)").unwrap(),
            intent: Intent::CodeAssist,
            confidence: 0.80,
        },
        // Schedule
        KeywordRule {
            pattern: Regex::new(r"(?i)(提醒|日程|日历|会议|预约|安排|定时|几点|明天|下周|周)").unwrap(),
            intent: Intent::Schedule,
            confidence: 0.85,
        },
        // Questions
        KeywordRule {
            pattern: Regex::new(r"(?i)(什么|怎么|如何|为什么|是什么|什么意思|解释)").unwrap(),
            intent: Intent::Question,
            confidence: 0.80,
        },
    ]
}
