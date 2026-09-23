"""Small deterministic requests; artificial cases are kept out of model outputs."""

import json


def question(kind, instructions="", criteria=None):
    result = dict(type=kind, instructions=instructions)
    if criteria is not None:
        result["criteria"] = criteria
    return result


def request(state, questions):
    return json.dumps(dict(state=state, questions=questions), ensure_ascii=False)


def model_cases(tok):
    zh = dict(
        choice=question("choice", "由哪个部门处理？", {"账单": "退款、支付", "技术": "软件故障", "销售": "购买咨询"}),
        score=question("score", "处理优先级？", ["低", "中", "高", "紧急"]),
        noul=question("noul", "客户需要退款。"))
    en = dict(
        choice=question("choice", "Choose a team.", {"billing": None, "technical": "Software faults", "sales": ""}),
        score=question("score", "Rate urgency.", ["low", "medium", "high"]),
        noul=question("noul", "The customer reports a software fault."))
    for language, state, questions in [("zh", "客户重复扣款，希望立即退款。", zh),
                                       ("en", "The app crashes when I sign in.", en)]:
        for kind, q in questions.items():
            yield f"{language}-{kind}", request(state, {kind: q})
    mixed = {f"{lang}-{key}": q for lang, qs in [("zh", zh), ("en", en)] for key, q in qs.items()}
    yield "mixed-6", request("客户需要退款。 The app crashes.", mixed)
    yield "mixed-16", request("客户需要帮助。", {str(i): list(mixed.values())[i % 6] for i in range(16)})
    for k in (2, 3, 5, 6, 10, 11, 32):
        yield f"options-{k}", request("test", {"q": question("choice", "Select.", [str(i) for i in range(k)])})
    yield "long-padding", request("客户需要帮助。 " * 1500, {
        "long-instructions": question("choice", "请仔细判断。 " * 400, ["a", "b"]),
        "long-options": question("score", "Rate.", ["candidate " * 70] * 11),
        "short-head": question("noul", "需要帮助。")})
    markers = tok.mask_token * 2 + " " + " ".join(tok.all_special_tokens) + " [MASK]"
    yield "marker", request(markers + " 😄", {
        "q": question("choice", markers, {tok.mask_token: tok.mask_token + " x", "": tok.sep_token}),
        "n": question("noul", "成立吗？", {"false": "", "true": "成立", "ignored": None})})
    yield "normalization", request(None, {
        "10": question("choice", True, ["b", "a", "b", ""]),
        "2": question("score", [], ["high", "low", "high"]),
        "": question("noul", {}, {"false": "不成立"})})
    value = r'{"10":1.0,"2":{"z":1,"a":" 空格\n\t\"\\ 中文😄","z":1e0},"10":1e0,"pair":"\ud83d\ude04","n":[-0,-0.0,1e-7,1e-4,1e15,1e16,1e400,-1e400,1e-4000,-1e-4000,9007199254740993,18446744073709551616,-9223372036854775809]}'
    yield "json-order-numbers", '{"state":' + value + ',"questions":{"q":{"type":"noul","instructions":' + value + '}}}'
    for name, value in [("huge-integer", "9" * 4301), ("scalars", "[null,true,false,{},[],1,1.0,1e0]")]:
        yield name, '{"state":' + value + ',"questions":{"q":{"type":"noul","instructions":' + value + '}}}'


def rejected_cases():
    template = '{"state":%s,"questions":{"q":{"type":"noul","instructions":null}}}'
    invalid = ["NaN", "Infinity", "-Infinity", "01", "+1", "1.", "1e", "[1,]",
               '{"x":1,}', '"\\ud800"', '{"\\udc00":null}', '"\n"']
    cases = [(f"syntax-{i}", template % value) for i, value in enumerate(invalid)]
    cases += [("overwritten-surrogate", (template % '"\\ud800"').replace(',"questions"', ',"state":null,"questions"')),
              ("unknown-surrogate", (template % "null")[:-1] + ',"ignored":"\\ud800"}'),
              ("trailing", template % "null" + " null"), ("bom", "\ufeff" + template % "null"),
              ("missing-state", '{"questions":{"q":{"type":"noul","instructions":null}}}'),
              ("criteria-null", request(None, {"q": {"type": "noul", "instructions": "", "criteria": None}})),
              ("one-option", request(None, {"q": question("choice", "", ["x", "x"])}))]
    rows = [dict(name=name, request_bytes=list(raw.encode()), status=400, code="invalid_request") for name, raw in cases]
    rows.append(dict(name="invalid-utf8", request_bytes=list((template % '"X"').encode().replace(b"X", b"\xff")),
                     status=400, code="invalid_request"))
    return dict(schema_version=1, kind="contract_rejection", provenance="docs/compatibility.md sections 2 and 7", cases=rows)
