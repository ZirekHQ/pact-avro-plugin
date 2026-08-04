package com.github.austek.example.pulsar.avro;

import static org.assertj.core.api.Assertions.assertThat;
import static org.junit.jupiter.api.Assertions.assertDoesNotThrow;

import au.com.dius.pact.consumer.dsl.PactBuilder;
import au.com.dius.pact.consumer.junit5.PactConsumerTestExt;
import au.com.dius.pact.consumer.junit5.PactTestFor;
import au.com.dius.pact.consumer.junit5.ProviderType;
import au.com.dius.pact.core.model.ContentTypeHint;
import au.com.dius.pact.core.model.PactSpecVersion;
import au.com.dius.pact.core.model.V4Interaction;
import au.com.dius.pact.core.model.V4Pact;
import au.com.dius.pact.core.model.annotations.Pact;
import au.com.dius.pact.core.model.matchingrules.MatchingRule;
import au.com.dius.pact.core.model.matchingrules.MatchingRuleCategory;
import au.com.dius.pact.core.model.matchingrules.MatchingRuleGroup;
import au.com.dius.pact.core.model.matchingrules.MatchingRulesImpl;
import au.com.dius.pact.core.model.v4.MessageContents;
import com.github.austek.example.Item;
import com.github.austek.example.Order;
import com.github.austek.example.Status;
import java.io.ByteArrayInputStream;
import java.io.IOException;
import java.net.URISyntaxException;
import java.nio.file.Paths;
import java.util.*;
import org.apache.avro.io.BinaryDecoder;
import org.apache.avro.io.DecoderFactory;
import org.apache.avro.specific.SpecificDatumReader;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;

@ExtendWith(PactConsumerTestExt.class)
@PactTestFor(
    providerName = "avro-plugin-provider",
    providerType = ProviderType.ASYNCH,
    pactVersion = PactSpecVersion.V4)
class PactPulsarConsumerTest {
  private final String schemasPath;
  private final OrderService orderService = new OrderService();

  PactPulsarConsumerTest() throws URISyntaxException {
    schemasPath =
        Paths.get(Objects.requireNonNull(getClass().getResource("/avro/orders.avsc")).toURI())
            .toFile()
            .getAbsolutePath();
  }

  @Pact(consumer = "avro-plugin-consumer")
  V4Pact configureRecordWithDependantRecord(PactBuilder builder) {
    // tag::configuration[]
    return builder
        .usingPlugin("avro")
        .expectsToReceive("Order Created", "core/interaction/message")
        .with(
            Map.of(
                "message.contents",
                Map.ofEntries(
                    Map.entry("pact:avro", schemasPath),
                    Map.entry("pact:record-name", "Order"),
                    Map.entry("pact:content-type", "avro/binary"),
                    Map.entry("id", "notEmpty('100')"),
                    Map.entry("names", "notEmpty('name-1')"),
                    Map.entry("enabled", "matching(boolean, true)"),
                    Map.entry("height", "matching(decimal, 15.8)"),
                    Map.entry("width", "matching(decimal, 1.8)"),
                    Map.entry("status", "matching(equalTo, 'CREATED')"),
                    Map.entry(
                        "address",
                        Map.of(
                            "no", "matching(integer, 121)",
                            "street", "matching(equalTo, 'street name')")),
                    Map.entry(
                        "items",
                        List.of(
                            Map.of(
                                "name", "notEmpty('Item-1')",
                                "id", "notEmpty('1')"),
                            Map.of(
                                "name", "notEmpty('Item-2')",
                                "id", "notEmpty('2')"))),
                    Map.entry("userId", "notEmpty('20bef962-8cbd-4b8c-8337-97ae385ac45d')"))))
        .toPact();
    // end::configuration[]
  }

  // tag::consumer_test[]
  @Test
  @PactTestFor(pactMethod = "configureRecordWithDependantRecord")
  void consumerRecordWithDependantRecord(V4Interaction.AsynchronousMessage message)
      throws IOException {
    MessageContents messageContents = message.getContents();
    List<Order> orders =
        arrayByteToAvroRecord(Order.class, messageContents.getContents().getValue());
    Order order = assertFirstOrder(orders);

    assertThat(messageContents.getContents().getContentType())
        .hasToString("avro/binary; record=Order");
    assertThat(messageContents.getContents().getContentTypeHint())
        .isEqualTo(ContentTypeHint.BINARY);

    Map<String, MatchingRuleCategory> ruleCategoryMap =
        ((MatchingRulesImpl) messageContents.getMatchingRules()).getRules();
    assertThat(ruleCategoryMap).hasSize(1);
    Map<String, MatchingRuleGroup> rules = ruleCategoryMap.get("body").getMatchingRules();
    List<MatchingRule> idRules = rules.get("$.id").getRules();
    assertThat(idRules).hasSize(1);
    assertThat(idRules.get(0)).extracting("name").isEqualTo("not-empty");
    List<MatchingRule> name0Rules = rules.get("$.names").getRules();
    assertThat(name0Rules).hasSize(1);
    assertThat(name0Rules.get(0)).extracting("name").isEqualTo("not-empty");

    assertThat(order.getUserId())
        .isEqualTo(UUID.fromString("20bef962-8cbd-4b8c-8337-97ae385ac45d"));

    assertDoesNotThrow(() -> orderService.process(order));
  }

  public static <T> List<T> arrayByteToAvroRecord(Class<T> c, byte[] bytes) throws IOException {
    SpecificDatumReader<T> datumReader = new SpecificDatumReader<>(c);
    List<T> records = new ArrayList<>();

    try (ByteArrayInputStream in = new ByteArrayInputStream(bytes)) {
      BinaryDecoder decoder = DecoderFactory.get().binaryDecoder(in, null);
      while (!decoder.isEnd()) records.add(datumReader.read(null, decoder));
    }

    return records;
  }

  // end::consumer_test[]

  private static Order assertFirstOrder(List<Order> orders) {
    assertThat(orders).hasSize(1);
    Order order = orders.get(0);
    assertThat(order.getId()).isEqualTo(100);
    assertThat(order.getNames()).hasToString("name-1");
    assertThat(order.getEnabled()).isTrue();
    assertThat(order.getHeight()).isEqualTo(15.8F);
    assertThat(order.getWidth()).isEqualTo(1.8D);
    assertThat(order.getStatus()).isEqualTo(Status.CREATED);
    assertThat(order.getAddress().getNo()).isEqualTo(121);
    assertThat(order.getAddress().getStreet()).hasToString("street name");
    assertThat(order.getAddress().getZipcode()).isNull();
    assertThat(order.getItems()).hasSize(2);
    Item item1 = order.getItems().get(0);
    assertThat(item1.getName()).hasToString("Item-1");
    assertThat(item1.getId()).isEqualTo(1L);
    Item item2 = order.getItems().get(1);
    assertThat(item2.getName()).hasToString("Item-2");
    assertThat(item2.getId()).isEqualTo(2L);
    return order;
  }
}
